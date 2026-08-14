use language_tags::LanguageTag;
use mp4_atom::{Codec, FourCC, Moof, Moov, Trak};
use relative_path::RelativePath;
use thiserror::Error;
use tokio_util::compat::FuturesAsyncReadCompatExt;

use crate::{
    codec_config::CodecConfig,
    frame_rate::FrameRate,
    mp4_box_reader::Mp4BoxReader,
    storage::Storage,
    track::{AudioMetadata, CmafTrack, TextMetadata, TextTrack, Track, VideoMetadata},
};

pub struct TrackResolver;

#[derive(Debug, Error)]
pub enum TrackResolverError {
    #[error("failed to resolve track")]
    Failed,
}

impl TrackResolver {
    pub async fn discover(path: &RelativePath) -> Result<Track, TrackResolverError> {
        match path.extension() {
            Some("mp4") => Self::discover_cmaf(path).await,
            Some("vtt") => Self::discover_web_vtt(path).await,
            _ => Err(TrackResolverError::Failed),
        }
    }

    async fn discover_cmaf(path: &RelativePath) -> Result<Track, TrackResolverError> {
        Ok(Self::read_cmaf_metadata(path).await?.into_track(path))
    }

    async fn discover_web_vtt(_path: &RelativePath) -> Result<Track, TrackResolverError> {
        Err(TrackResolverError::Failed)
    }

    async fn read_cmaf_metadata(
        path: &RelativePath,
    ) -> Result<CmafTrackMetadata, TrackResolverError> {
        let mut reader = Storage::source_op()
            .map_err(|_| TrackResolverError::Failed)?
            .reader(path.as_str())
            .await
            .map_err(|_| TrackResolverError::Failed)?
            .into_futures_async_read(..)
            .await
            .map_err(|_| TrackResolverError::Failed)?;

        let mut reader = Mp4BoxReader::new(reader.compat());
        let moov = reader
            .read_box::<Moov>()
            .await
            .map_err(|_| TrackResolverError::Failed)?;
        let first_moof = reader
            .read_box::<Moof>()
            .await
            .map_err(|_| TrackResolverError::Failed)?;

        Self::cmaf_metadata(&moov, &first_moof)
    }

    fn cmaf_metadata(
        moov: &Moov,
        first_moof: &Moof,
    ) -> Result<CmafTrackMetadata, TrackResolverError> {
        let [track] = moov.trak.as_slice() else {
            return Err(TrackResolverError::Failed);
        };
        let [codec] = track.mdia.minf.stbl.stsd.codecs.as_slice() else {
            return Err(TrackResolverError::Failed);
        };

        match track.mdia.hdlr.handler {
            handler if handler == FourCC::new(b"vide") => {
                let (width, height) = Self::video_dimensions(codec)?;
                let frame_rate = Self::frame_rate(moov, first_moof, track)?;

                Ok(CmafTrackMetadata::Video {
                    codec: Self::codec_config(codec)?,
                    metadata: VideoMetadata {
                        width,
                        height,
                        frame_rate,
                    },
                })
            }
            handler if handler == FourCC::new(b"soun") => {
                let (sample_rate, channels) = Self::audio_properties(codec)?;

                Ok(CmafTrackMetadata::Audio {
                    codec: Self::codec_config(codec)?,
                    metadata: AudioMetadata {
                        sample_rate,
                        channels,
                        language: Self::language(track)?,
                        role: None,
                    },
                })
            }
            handler if handler == FourCC::new(b"text") || handler == FourCC::new(b"subt") => {
                Ok(CmafTrackMetadata::Text {
                    codec: Self::codec_config(codec)?,
                    metadata: TextMetadata {
                        language: Self::language(track)?,
                        role: None,
                    },
                })
            }
            _ => Err(TrackResolverError::Failed),
        }
    }

    fn codec_config(codec: &Codec) -> Result<CodecConfig, TrackResolverError> {
        CodecConfig::from_atom(codec).map_err(|_| TrackResolverError::Failed)
    }

    fn video_dimensions(codec: &Codec) -> Result<(u32, u32), TrackResolverError> {
        let (width, height) = match codec {
            Codec::Avc1(codec) => (codec.visual.width, codec.visual.height),
            Codec::Hev1(codec) => (codec.visual.width, codec.visual.height),
            Codec::Hvc1(codec) => (codec.visual.width, codec.visual.height),
            _ => return Err(TrackResolverError::Failed),
        };

        Ok((u32::from(width), u32::from(height)))
    }

    fn audio_properties(codec: &Codec) -> Result<(u32, u16), TrackResolverError> {
        let audio = match codec {
            Codec::Mp4a(codec) => &codec.audio,
            Codec::Ac3(codec) => &codec.audio,
            Codec::Eac3(codec) => &codec.audio,
            _ => return Err(TrackResolverError::Failed),
        };

        Ok((u32::from(audio.sample_rate.integer()), audio.channel_count))
    }

    fn frame_rate(
        moov: &Moov,
        first_moof: &Moof,
        track: &Trak,
    ) -> Result<FrameRate, TrackResolverError> {
        let traf = first_moof
            .traf
            .iter()
            .find(|traf| traf.tfhd.track_id == track.tkhd.track_id)
            .ok_or(TrackResolverError::Failed)?;
        let default_sample_duration = traf.tfhd.default_sample_duration.or_else(|| {
            moov.mvex
                .as_ref()
                .and_then(|mvex| {
                    mvex.trex
                        .iter()
                        .find(|trex| trex.track_id == track.tkhd.track_id)
                })
                .map(|trex| trex.default_sample_duration)
        });
        let timescale = track.mdia.mdhd.timescale;

        if timescale == 0 {
            return Err(TrackResolverError::Failed);
        }

        let sample_duration = Self::constant_sample_duration(traf, default_sample_duration)?;
        if sample_duration == 0 {
            return Err(TrackResolverError::Failed);
        }
        let divisor = greatest_common_divisor(timescale, sample_duration);
        FrameRate::new(timescale / divisor, sample_duration / divisor)
            .map_err(|_| TrackResolverError::Failed)
    }

    fn constant_sample_duration(
        traf: &mp4_atom::Traf,
        default_sample_duration: Option<u32>,
    ) -> Result<u32, TrackResolverError> {
        let mut sample_duration = None;

        for trun in &traf.trun {
            for entry in &trun.entries {
                let duration = entry
                    .duration
                    .or(default_sample_duration)
                    .ok_or(TrackResolverError::Failed)?;

                if let Some(sample_duration) = sample_duration {
                    if sample_duration != duration {
                        return Err(TrackResolverError::Failed);
                    }
                } else {
                    sample_duration = Some(duration);
                }
            }
        }

        sample_duration.ok_or(TrackResolverError::Failed)
    }

    fn language(track: &Trak) -> Result<LanguageTag, TrackResolverError> {
        track
            .mdia
            .mdhd
            .language
            .parse()
            .map_err(|_| TrackResolverError::Failed)
    }
}

enum CmafTrackMetadata {
    Video {
        codec: CodecConfig,
        metadata: VideoMetadata,
    },
    Audio {
        codec: CodecConfig,
        metadata: AudioMetadata,
    },
    Text {
        codec: CodecConfig,
        metadata: TextMetadata,
    },
}

impl CmafTrackMetadata {
    fn into_track(self, path: &RelativePath) -> Track {
        match self {
            Self::Video { codec, metadata } => Track::Video(CmafTrack {
                path: path.as_str().into(),
                codec,
                metadata,
            }),
            Self::Audio { codec, metadata } => Track::Audio(CmafTrack {
                path: path.as_str().into(),
                codec,
                metadata,
            }),
            Self::Text { codec, metadata } => Track::Text(TextTrack::Cmaf(CmafTrack {
                path: path.as_str().into(),
                codec,
                metadata,
            })),
        }
    }
}

fn greatest_common_divisor(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        (left, right) = (right, left % right);
    }

    left
}
