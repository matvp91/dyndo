use futures_util::io::AsyncRead;
use language_tags::LanguageTag;
use mp4_atom::{Codec, Error as Mp4AtomError, FourCC, Moof, Moov, Traf, Trak};
use relative_path::{RelativePath, RelativePathBuf};
use thiserror::Error;
use tokio_util::compat::FuturesAsyncReadCompatExt;

use super::{
    AudioMetadata, CmafAudioTrack, CmafTrack, CmafTextTrack, CmafVideoTrack, TextMetadata,
    Track, VideoMetadata, WebVttTextTrack,
};
use crate::{
    codec_config::CodecConfig, frame_rate::FrameRate, mp4_box_reader::Mp4BoxReader,
    mp4_readable::Mp4Readable,
    storage::{Storage, StorageError},
};

#[derive(Debug, Error)]
pub enum DiscoverError {
    #[error("unsupported track format")]
    UnsupportedFormat,
    #[error("failed to access source: {0}")]
    Storage(#[from] StorageError),
    #[error("failed to read source: {0}")]
    Source(#[from] opendal::Error),
    #[error("failed to read MP4: {0}")]
    Mp4(#[from] Mp4AtomError),
    #[error("invalid CMAF track")]
    InvalidCmaf,
}

impl Track {
    pub async fn discover(path: &RelativePath) -> Result<Self, DiscoverError> {
        let mut reader = Storage::source_op()?
            .reader(path.as_str())
            .await?
            .into_futures_async_read(..)
            .await?;

        match path.extension() {
            Some("mp4") => {
                let track = DiscoveredCmafTrack::from_reader(&mut reader).await?;

                Ok(track.into_track(path.as_str().into()))
            }
            Some("vtt") => Ok(Self::WebVttText(WebVttTextTrack {
                path: path.as_str().into(),
                metadata: TextMetadata {
                    language: super::language_und(),
                    role: None,
                },
            })),
            _ => Err(DiscoverError::UnsupportedFormat),
        }
    }
}

enum DiscoveredCmafTrack {
    Video(CmafVideoTrack),
    Audio(CmafAudioTrack),
    Text(CmafTextTrack),
}

impl DiscoveredCmafTrack {
    fn new(moov: &Moov, first_moof: &Moof) -> Result<Self, DiscoverError> {
        let [track] = moov.trak.as_slice() else {
            return Err(DiscoverError::InvalidCmaf);
        };
        let [codec] = track.mdia.minf.stbl.stsd.codecs.as_slice() else {
            return Err(DiscoverError::InvalidCmaf);
        };
        let codec_config = CodecConfig::from_atom(codec).map_err(|_| DiscoverError::InvalidCmaf)?;
        let path = RelativePathBuf::from("");

        match track.mdia.hdlr.handler {
            handler if handler == FourCC::new(b"vide") => Ok(Self::Video(map_video(
                path, codec_config, codec, moov, first_moof, track,
            )?)),
            handler if handler == FourCC::new(b"soun") => {
                Ok(Self::Audio(map_audio(path, codec_config, codec, track)?))
            }
            handler if handler == FourCC::new(b"text") || handler == FourCC::new(b"subt") => {
                Ok(Self::Text(map_text(path, codec_config, track)?))
            }
            _ => Err(DiscoverError::InvalidCmaf),
        }
    }

    fn into_track(self, path: RelativePathBuf) -> Track {
        match self {
            Self::Video(track) => Track::CmafVideo(with_path(track, path)),
            Self::Audio(track) => Track::CmafAudio(with_path(track, path)),
            Self::Text(track) => Track::CmafText(with_path(track, path)),
        }
    }
}

fn with_path<M>(mut track: CmafTrack<M>, path: RelativePathBuf) -> CmafTrack<M> {
    track.path = path;
    track
}

impl Mp4Readable for DiscoveredCmafTrack {
    type Error = DiscoverError;

    async fn from_reader(reader: &mut (impl AsyncRead + Unpin)) -> Result<Self, Self::Error> {
        let mut reader = Mp4BoxReader::new(reader.compat());
        let moov = reader.read_box::<Moov>().await?;
        let first_moof = reader.read_box::<Moof>().await?;

        Self::new(&moov, &first_moof)
    }
}

fn map_video(
    path: RelativePathBuf,
    codec_config: CodecConfig,
    codec: &Codec,
    moov: &Moov,
    first_moof: &Moof,
    track: &Trak,
) -> Result<CmafVideoTrack, DiscoverError> {
    let (width, height) = video_dimensions(codec)?;
    let frame_rate = frame_rate(moov, first_moof, track)?;

    Ok(CmafTrack {
        path,
        codec: codec_config,
        metadata: VideoMetadata {
            width,
            height,
            frame_rate,
        },
    })
}

fn map_audio(
    path: RelativePathBuf,
    codec_config: CodecConfig,
    codec: &Codec,
    track: &Trak,
) -> Result<CmafAudioTrack, DiscoverError> {
    let (sample_rate, channels) = audio_properties(codec)?;

    Ok(CmafTrack {
        path,
        codec: codec_config,
        metadata: AudioMetadata {
            sample_rate,
            channels,
            language: language(track)?,
            role: None,
        },
    })
}

fn map_text(
    path: RelativePathBuf,
    codec_config: CodecConfig,
    track: &Trak,
) -> Result<CmafTextTrack, DiscoverError> {
    Ok(CmafTrack {
        path,
        codec: codec_config,
        metadata: TextMetadata {
            language: language(track)?,
            role: None,
        },
    })
}

fn video_dimensions(codec: &Codec) -> Result<(u32, u32), DiscoverError> {
    let (width, height) = match codec {
        Codec::Avc1(codec) => (codec.visual.width, codec.visual.height),
        Codec::Hev1(codec) => (codec.visual.width, codec.visual.height),
        Codec::Hvc1(codec) => (codec.visual.width, codec.visual.height),
        _ => return Err(DiscoverError::InvalidCmaf),
    };

    Ok((u32::from(width), u32::from(height)))
}

fn audio_properties(codec: &Codec) -> Result<(u32, u16), DiscoverError> {
    let audio = match codec {
        Codec::Mp4a(codec) => &codec.audio,
        Codec::Ac3(codec) => &codec.audio,
        Codec::Eac3(codec) => &codec.audio,
        _ => return Err(DiscoverError::InvalidCmaf),
    };

    Ok((u32::from(audio.sample_rate.integer()), audio.channel_count))
}

fn frame_rate(
    moov: &Moov,
    moof: &Moof,
    track: &Trak,
) -> Result<FrameRate, DiscoverError> {
    let track_id = track.tkhd.track_id;
    let timescale = track.mdia.mdhd.timescale;

    if timescale == 0 {
        return Err(DiscoverError::InvalidCmaf);
    }

    let traf = moof
        .traf
        .iter()
        .find(|traf| traf.tfhd.track_id == track_id)
        .ok_or(DiscoverError::InvalidCmaf)?;

    let default_duration = traf.tfhd.default_sample_duration.or_else(|| {
        moov.mvex.as_ref()?
            .trex
            .iter()
            .find(|trex| trex.track_id == track_id)
            .map(|trex| trex.default_sample_duration)
    });

    let duration = traf
        .trun
        .iter()
        .flat_map(|trun| &trun.entries)
        .next()
        .and_then(|entry| entry.duration.or(default_duration))
        .filter(|&duration| duration != 0)
        .ok_or(DiscoverError::InvalidCmaf)?;

    let gcd = greatest_common_divisor(timescale, duration);

    FrameRate::new(timescale / gcd, duration / gcd)
        .map_err(|_| DiscoverError::InvalidCmaf)
}

fn language(track: &Trak) -> Result<LanguageTag, DiscoverError> {
    track
        .mdia
        .mdhd
        .language
        .parse()
        .map_err(|_| DiscoverError::InvalidCmaf)
}

fn greatest_common_divisor(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        (left, right) = (right, left % right);
    }

    left
}
