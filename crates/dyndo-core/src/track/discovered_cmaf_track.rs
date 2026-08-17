use futures_util::io::AsyncRead;
use language_tags::LanguageTag;
use mp4_atom::{Codec, FourCC, Moof, Moov, Trak};
use relative_path::RelativePathBuf;
use tokio_util::compat::FuturesAsyncReadCompatExt;

use super::{
    AudioMetadata, CmafTrack, TextMetadata, TextTrack, Track, VideoMetadata,
};
use crate::{
    codec_config::CodecConfig,
    frame_rate::FrameRate,
    mp4_box_reader::Mp4BoxReader,
    mp4_readable::Mp4Readable,
};
use super::track_discover::DiscoverError;

pub(super) struct DiscoveredCmafTrack {
    codec: CodecConfig,
    metadata: CmafTrackMetadata,
}

enum CmafTrackMetadata {
    Video(VideoMetadata),
    Audio(AudioMetadata),
    Text(TextMetadata),
}

impl DiscoveredCmafTrack {
    fn from_boxes(moov: &Moov, first_moof: &Moof) -> Result<Self, DiscoverError> {
        let [track] = moov.trak.as_slice() else {
            return Err(DiscoverError::InvalidCmaf(
                "initialization segment must contain exactly one track".into(),
            ));
        };
        let [codec] = track.mdia.minf.stbl.stsd.codecs.as_slice() else {
            return Err(DiscoverError::InvalidCmaf(
                "track must contain exactly one codec".into(),
            ));
        };
        let codec_config = CodecConfig::from_atom(codec).map_err(|_| {
            DiscoverError::InvalidCmaf("track has an unsupported codec configuration".into())
        })?;
        let metadata = match track.mdia.hdlr.handler {
            handler if handler == FourCC::new(b"vide") => {
                CmafTrackMetadata::Video(map_video(codec, moov, first_moof, track)?)
            }
            handler if handler == FourCC::new(b"soun") => {
                CmafTrackMetadata::Audio(map_audio(codec, track)?)
            }
            handler if handler == FourCC::new(b"text") || handler == FourCC::new(b"subt") => {
                CmafTrackMetadata::Text(map_text(track)?)
            }
            handler => {
                return Err(DiscoverError::InvalidCmaf(format!(
                    "unsupported track handler: {handler}"
                )));
            }
        };

        Ok(Self {
            codec: codec_config,
            metadata,
        })
    }

    pub(super) fn into_track(self, path: RelativePathBuf) -> Track {
        match self.metadata {
            CmafTrackMetadata::Video(metadata) => Track::Video(CmafTrack {
                path,
                codec: self.codec,
                metadata,
            }),
            CmafTrackMetadata::Audio(metadata) => Track::Audio(CmafTrack {
                path,
                codec: self.codec,
                metadata,
            }),
            CmafTrackMetadata::Text(metadata) => Track::Text(TextTrack::Cmaf(CmafTrack {
                path,
                codec: self.codec,
                metadata,
            })),
        }
    }
}

impl Mp4Readable for DiscoveredCmafTrack {
    type Error = DiscoverError;

    async fn from_reader(reader: &mut (impl AsyncRead + Unpin)) -> Result<Self, Self::Error> {
        let mut reader = Mp4BoxReader::new(reader.compat());
        let moov = reader.read_box::<Moov>().await?;
        let first_moof = reader.read_box::<Moof>().await?;

        Self::from_boxes(&moov, &first_moof)
    }
}

fn map_video(
    codec: &Codec,
    moov: &Moov,
    first_moof: &Moof,
    track: &Trak,
) -> Result<VideoMetadata, DiscoverError> {
    let (width, height) = video_dimensions(codec)?;
    let frame_rate = frame_rate(moov, first_moof, track)?;

    Ok(VideoMetadata {
        width,
        height,
        frame_rate,
    })
}

fn map_audio(
    codec: &Codec,
    track: &Trak,
) -> Result<AudioMetadata, DiscoverError> {
    let (sample_rate, channels) = audio_properties(codec)?;

    Ok(AudioMetadata {
        sample_rate,
        channels,
        language: language(track)?,
        role: None,
    })
}

fn map_text(track: &Trak) -> Result<TextMetadata, DiscoverError> {
    Ok(TextMetadata {
        language: language(track)?,
        role: None,
    })
}

fn video_dimensions(codec: &Codec) -> Result<(u32, u32), DiscoverError> {
    let (width, height) = match codec {
        Codec::Avc1(codec) => (codec.visual.width, codec.visual.height),
        Codec::Hev1(codec) => (codec.visual.width, codec.visual.height),
        Codec::Hvc1(codec) => (codec.visual.width, codec.visual.height),
        _ => {
            return Err(DiscoverError::InvalidCmaf(
                "video track has an unsupported codec".into(),
            ));
        }
    };

    Ok((u32::from(width), u32::from(height)))
}

fn audio_properties(codec: &Codec) -> Result<(u32, u16), DiscoverError> {
    let audio = match codec {
        Codec::Mp4a(codec) => &codec.audio,
        Codec::Ac3(codec) => &codec.audio,
        Codec::Eac3(codec) => &codec.audio,
        _ => {
            return Err(DiscoverError::InvalidCmaf(
                "audio track has an unsupported codec".into(),
            ));
        }
    };

    Ok((u32::from(audio.sample_rate.integer()), audio.channel_count))
}

fn frame_rate(moov: &Moov, moof: &Moof, track: &Trak) -> Result<FrameRate, DiscoverError> {
    let track_id = track.tkhd.track_id;
    let timescale = track.mdia.mdhd.timescale;

    if timescale == 0 {
        return Err(DiscoverError::InvalidCmaf(
            "track timescale cannot be zero".into(),
        ));
    }

    let traf = moof
        .traf
        .iter()
        .find(|traf| traf.tfhd.track_id == track_id)
        .ok_or_else(|| {
            DiscoverError::InvalidCmaf(format!(
                "media fragment has no track fragment for track ID {track_id}"
            ))
        })?;

    let default_duration = traf.tfhd.default_sample_duration.or_else(|| {
        moov.mvex
            .as_ref()?
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
        .ok_or_else(|| {
            DiscoverError::InvalidCmaf("track has no non-zero sample duration".into())
        })?;

    let gcd = greatest_common_divisor(timescale, duration);

    FrameRate::new(timescale / gcd, duration / gcd)
        .map_err(|_| DiscoverError::InvalidCmaf("invalid frame rate".into()))
}

fn language(track: &Trak) -> Result<LanguageTag, DiscoverError> {
    track
        .mdia
        .mdhd
        .language
        .parse()
        .map_err(|_| DiscoverError::InvalidCmaf("invalid track language".into()))
}

fn greatest_common_divisor(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        (left, right) = (right, left % right);
    }

    left
}
