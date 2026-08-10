use language_tags::LanguageTag;
use mp4_atom::{Codec as Mp4Codec, FourCC};

use super::super::box_reader::Boxes;
use super::super::codec::{AacCodec, Av1Codec, AvcCodec, CodecConfig, HevcCodec};
use super::super::track_kind::{AudioKind, TextKind, TrackKind, VideoKind, undetermined_language};
use super::TrackProberError;

pub(super) fn build_codec(boxes: &Boxes) -> Result<CodecConfig, TrackProberError> {
    let codec = &boxes.moov.trak[0].mdia.minf.stbl.stsd.codecs[0];
    match codec {
        Mp4Codec::Avc1(entry) => Ok(CodecConfig::Avc(AvcCodec::new(entry))),
        Mp4Codec::Av01(entry) => Ok(CodecConfig::Av1(Av1Codec::new(entry))),
        Mp4Codec::Hvc1(entry) => Ok(CodecConfig::Hevc(HevcCodec::new(entry))),
        Mp4Codec::Hev1(entry) => Ok(CodecConfig::Hevc(HevcCodec::new(entry))),
        Mp4Codec::Mp4a(entry) => Ok(CodecConfig::Aac(AacCodec::new(entry))),
        codec => Err(TrackProberError::UnsupportedCodec(codec_name(codec))),
    }
}

pub(super) fn build_kind(boxes: &Boxes) -> Result<TrackKind, TrackProberError> {
    let handler = boxes.moov.trak[0].mdia.hdlr.handler;

    if handler == FourCC::new(b"vide") {
        build_video_kind(boxes).map(TrackKind::Video)
    } else if handler == FourCC::new(b"soun") {
        build_audio_kind(boxes).map(TrackKind::Audio)
    } else if handler == FourCC::new(b"text") {
        Ok(TrackKind::Text(build_text_kind(boxes)))
    } else {
        Err(TrackProberError::UnsupportedTrackHandler)
    }
}

fn build_video_kind(boxes: &Boxes) -> Result<VideoKind, TrackProberError> {
    let sample_entry = &boxes.moov.trak[0].mdia.minf.stbl.stsd.codecs[0];
    let visual = match sample_entry {
        Mp4Codec::Avc1(entry) => &entry.visual,
        Mp4Codec::Av01(entry) => &entry.visual,
        Mp4Codec::Hvc1(entry) => &entry.visual,
        Mp4Codec::Hev1(entry) => &entry.visual,
        _ => return Err(TrackProberError::UnsupportedVideoSampleEntry),
    };

    Ok(VideoKind {
        width: u32::from(visual.width),
        height: u32::from(visual.height),
        frame_rate: frame_rate(boxes)?,
    })
}

fn build_audio_kind(boxes: &Boxes) -> Result<AudioKind, TrackProberError> {
    let sample_entry = &boxes.moov.trak[0].mdia.minf.stbl.stsd.codecs[0];
    let audio = match sample_entry {
        Mp4Codec::Mp4a(entry) => &entry.audio,
        Mp4Codec::Ac3(entry) => &entry.audio,
        Mp4Codec::Eac3(entry) => &entry.audio,
        _ => return Err(TrackProberError::UnsupportedAudioSampleEntry),
    };

    Ok(AudioKind {
        sample_rate: audio.sample_rate.integer() as u32,
        channels: audio.channel_count,
        language: language(boxes),
        role: None,
    })
}

fn build_text_kind(boxes: &Boxes) -> TextKind {
    TextKind {
        language: language(boxes),
        role: None,
    }
}

fn frame_rate(boxes: &Boxes) -> Result<String, TrackProberError> {
    let track = &boxes.moov.trak[0];
    let sample_duration = boxes
        .moof
        .traf
        .first()
        .and_then(|fragment| fragment.trun.first())
        .and_then(|run| run.entries.first())
        .and_then(|sample| sample.duration)
        .filter(|duration| *duration != 0)
        .ok_or(TrackProberError::MissingFrameRate)?;
    let timescale = track.mdia.mdhd.timescale;
    if timescale == 0 {
        return Err(TrackProberError::MissingFrameRate);
    }
    let divisor = greatest_common_divisor(timescale, sample_duration);

    Ok(format!(
        "{}/{}",
        timescale / divisor,
        sample_duration / divisor
    ))
}

fn greatest_common_divisor(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

fn language(boxes: &Boxes) -> LanguageTag {
    let language = boxes.moov.trak[0].mdia.mdhd.language.as_str();
    language.parse().unwrap_or_else(|_| undetermined_language())
}

fn codec_name(codec: &Mp4Codec) -> String {
    format!("{codec:?}")
        .split(['(', ' ', '{'])
        .next()
        .unwrap_or("unknown")
        .to_string()
}
