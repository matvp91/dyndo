use language_tags::LanguageTag;
use mp4_atom::{Codec, FourCC, Moof, Moov, Trak};

use super::{AudioMetadata, TextMetadata, VideoMetadata};
use crate::{
    codec_config::CodecConfig,
    frame_rate::FrameRate,
    mp4_box_reader::Mp4BoxReader,
    mp4_readable::{Mp4Readable, Mp4ReadableError},
};

pub(super) enum CmafMetadata {
    Video(VideoMetadata),
    Audio(AudioMetadata),
    Text(TextMetadata),
}

impl Mp4Readable for CmafMetadata {
    type Error = Mp4ReadableError;
    type Output = (CodecConfig, Self);

    async fn from_mp4_reader(
        reader: &mut Mp4BoxReader<impl tokio::io::AsyncRead + Unpin>,
    ) -> Result<Self::Output, Self::Error> {
        let moov = reader.read_box::<Moov>().await?;
        let first_moof = reader.read_box::<Moof>().await?;

        let [track] = moov.trak.as_slice() else {
            return Err(Mp4ReadableError::invalid(
                "initialization segment must contain exactly one track",
            ));
        };
        let [codec] = track.mdia.minf.stbl.stsd.codecs.as_slice() else {
            return Err(Mp4ReadableError::invalid(
                "track must contain exactly one codec",
            ));
        };
        let codec_config = CodecConfig::from_atom(codec).map_err(|_| {
            Mp4ReadableError::invalid("track has an unsupported codec configuration")
        })?;

        let metadata = match track.mdia.hdlr.handler {
            handler if handler == FourCC::new(b"vide") => {
                Self::Video(map_video(codec, &moov, &first_moof, track)?)
            }
            handler if handler == FourCC::new(b"soun") => Self::Audio(map_audio(codec, track)?),
            handler if handler == FourCC::new(b"text") || handler == FourCC::new(b"subt") => {
                Self::Text(map_text(track)?)
            }
            handler => {
                return Err(Mp4ReadableError::invalid(format!(
                    "unsupported track handler: {handler}"
                )));
            }
        };

        Ok((codec_config, metadata))
    }
}

fn map_video(
    codec: &Codec,
    moov: &Moov,
    first_moof: &Moof,
    track: &Trak,
) -> Result<VideoMetadata, Mp4ReadableError> {
    let (width, height) = video_dimensions(codec)?;
    let frame_rate = frame_rate(moov, first_moof, track)?;

    Ok(VideoMetadata {
        width,
        height,
        frame_rate,
    })
}

fn map_audio(codec: &Codec, track: &Trak) -> Result<AudioMetadata, Mp4ReadableError> {
    let (sample_rate, channels) = audio_properties(codec)?;

    Ok(AudioMetadata {
        sample_rate,
        channels,
        language: language(track)?,
        role: None,
    })
}

fn map_text(track: &Trak) -> Result<TextMetadata, Mp4ReadableError> {
    Ok(TextMetadata {
        language: language(track)?,
        role: None,
    })
}

fn video_dimensions(codec: &Codec) -> Result<(u32, u32), Mp4ReadableError> {
    let (width, height) = match codec {
        Codec::Avc1(codec) => (codec.visual.width, codec.visual.height),
        Codec::Hev1(codec) => (codec.visual.width, codec.visual.height),
        Codec::Hvc1(codec) => (codec.visual.width, codec.visual.height),
        _ => {
            return Err(Mp4ReadableError::invalid(
                "video track has an unsupported codec",
            ));
        }
    };

    Ok((u32::from(width), u32::from(height)))
}

fn audio_properties(codec: &Codec) -> Result<(u32, u16), Mp4ReadableError> {
    let audio = match codec {
        Codec::Mp4a(codec) => &codec.audio,
        Codec::Ac3(codec) => &codec.audio,
        Codec::Eac3(codec) => &codec.audio,
        _ => {
            return Err(Mp4ReadableError::invalid(
                "audio track has an unsupported codec",
            ));
        }
    };

    Ok((u32::from(audio.sample_rate.integer()), audio.channel_count))
}

fn frame_rate(moov: &Moov, moof: &Moof, track: &Trak) -> Result<FrameRate, Mp4ReadableError> {
    let track_id = track.tkhd.track_id;
    let timescale = track.mdia.mdhd.timescale;

    if timescale == 0 {
        return Err(Mp4ReadableError::invalid("track timescale cannot be zero"));
    }

    let traf = moof
        .traf
        .iter()
        .find(|traf| traf.tfhd.track_id == track_id)
        .ok_or_else(|| {
            Mp4ReadableError::invalid(format!(
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
        .ok_or_else(|| Mp4ReadableError::invalid("track has no non-zero sample duration"))?;

    let gcd = greatest_common_divisor(timescale, duration);

    FrameRate::new(timescale / gcd, duration / gcd)
        .map_err(|_| Mp4ReadableError::invalid("invalid frame rate"))
}

fn language(track: &Trak) -> Result<LanguageTag, Mp4ReadableError> {
    track
        .mdia
        .mdhd
        .language
        .parse()
        .map_err(|_| Mp4ReadableError::invalid("invalid track language"))
}

fn greatest_common_divisor(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        (left, right) = (right, left % right);
    }

    left
}
