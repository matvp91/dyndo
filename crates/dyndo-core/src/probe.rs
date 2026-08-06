use std::ops::Range;

use language_tags::LanguageTag;
use mp4_atom::{Codec, FourCC, Hvcc};
use opendal::Operator;
use relative_path::RelativePath;

use crate::asset_descriptor::{AudioKind, TextKind, TrackKind, VideoKind, undetermined_language};
use crate::box_reader::{self, BoxReaderError, Boxes};
use crate::track::Fragment;

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error(transparent)]
    BoxReader(#[from] BoxReaderError),
    #[error(transparent)]
    Storage(#[from] opendal::Error),
    #[error("unsupported video sample entry")]
    UnsupportedVideoSampleEntry,
    #[error("unsupported audio sample entry")]
    UnsupportedAudioSampleEntry,
    #[error("unsupported track handler")]
    UnsupportedTrackHandler,
    #[error("video track has no sample duration")]
    MissingFrameRate,
    #[error("unsupported codec {0}")]
    UnsupportedCodec(String),
    #[error("segment offset overflows")]
    SegmentOffsetOverflow,
    #[error("segment range overflows")]
    SegmentRangeOverflow,
}

pub(crate) struct Probed {
    pub codec: String,
    pub kind: TrackKind,
    pub timescale: u32,
    pub earliest_presentation_time: u64,
    pub initialization_range: Range<u64>,
    pub fragments: Vec<Fragment>,
}

pub(crate) async fn probe(op: &Operator, path: &RelativePath) -> Result<Probed, ProbeError> {
    let boxes = box_reader::scan(op, path.as_str()).await?;

    Ok(Probed {
        codec: codec(&boxes)?,
        kind: kind(&boxes)?,
        timescale: boxes.sidx.timescale,
        earliest_presentation_time: boxes.sidx.earliest_presentation_time,
        initialization_range: 0..boxes.moov_end,
        fragments: fragments(&boxes)?,
    })
}

fn codec(boxes: &Boxes) -> Result<String, ProbeError> {
    let codec = &boxes.moov.trak[0].mdia.minf.stbl.stsd.codecs[0];
    match codec {
        Codec::Avc1(entry) => Ok(format!(
            "avc1.{:02x}{:02x}{:02x}",
            entry.avcc.avc_profile_indication,
            entry.avcc.profile_compatibility,
            entry.avcc.avc_level_indication
        )),
        Codec::Av01(entry) => {
            let tier = if entry.av1c.seq_tier_0 { 'H' } else { 'M' };
            let bit_depth = if entry.av1c.twelve_bit {
                12
            } else if entry.av1c.high_bitdepth {
                10
            } else {
                8
            };
            Ok(format!(
                "av01.{}.{:02}{tier}.{bit_depth:02}",
                entry.av1c.seq_profile, entry.av1c.seq_level_idx_0
            ))
        }
        Codec::Hvc1(entry) => Ok(hevc_codec("hvc1", &entry.hvcc)),
        Codec::Hev1(entry) => Ok(hevc_codec("hev1", &entry.hvcc)),
        Codec::Mp4a(entry) => Ok(format!(
            "mp4a.40.{}",
            entry.esds.es_desc.dec_config.dec_specific.profile
        )),
        Codec::Ac3(_) => Ok("ac-3".to_string()),
        Codec::Eac3(_) => Ok("ec-3".to_string()),
        Codec::Wvtt(_) => Ok("wvtt".to_string()),
        codec => Err(ProbeError::UnsupportedCodec(codec_name(codec))),
    }
}

fn kind(boxes: &Boxes) -> Result<TrackKind, ProbeError> {
    let track = &boxes.moov.trak[0];
    let handler = track.mdia.hdlr.handler;
    let sample_entry = &track.mdia.minf.stbl.stsd.codecs[0];

    if handler == FourCC::new(b"vide") {
        let visual = match sample_entry {
            Codec::Avc1(entry) => &entry.visual,
            Codec::Av01(entry) => &entry.visual,
            Codec::Hvc1(entry) => &entry.visual,
            Codec::Hev1(entry) => &entry.visual,
            _ => return Err(ProbeError::UnsupportedVideoSampleEntry),
        };
        Ok(TrackKind::Video(VideoKind {
            width: u32::from(visual.width),
            height: u32::from(visual.height),
            frame_rate: frame_rate(boxes)?,
        }))
    } else if handler == FourCC::new(b"soun") {
        let audio = match sample_entry {
            Codec::Mp4a(entry) => &entry.audio,
            Codec::Ac3(entry) => &entry.audio,
            Codec::Eac3(entry) => &entry.audio,
            _ => return Err(ProbeError::UnsupportedAudioSampleEntry),
        };
        Ok(TrackKind::Audio(AudioKind {
            sample_rate: audio.sample_rate.integer() as u32,
            channels: audio.channel_count,
            language: language(boxes),
            role: None,
        }))
    } else if handler == FourCC::new(b"text") {
        Ok(TrackKind::Text(TextKind {
            language: language(boxes),
            role: None,
        }))
    } else {
        Err(ProbeError::UnsupportedTrackHandler)
    }
}

fn frame_rate(boxes: &Boxes) -> Result<String, ProbeError> {
    let track = &boxes.moov.trak[0];
    let track_id = track.tkhd.track_id;
    let fragment = boxes
        .moof
        .traf
        .iter()
        .find(|fragment| fragment.tfhd.track_id == track_id)
        .ok_or(ProbeError::MissingFrameRate)?;
    let sample_duration = fragment
        .trun
        .iter()
        .flat_map(|run| &run.entries)
        .next()
        .and_then(|sample| sample.duration)
        .or(fragment.tfhd.default_sample_duration)
        .or_else(|| {
            boxes
                .moov
                .mvex
                .as_ref()
                .and_then(|extensions| {
                    extensions
                        .trex
                        .iter()
                        .find(|defaults| defaults.track_id == track_id)
                })
                .map(|defaults| defaults.default_sample_duration)
        })
        .filter(|duration| *duration != 0)
        .ok_or(ProbeError::MissingFrameRate)?;
    let timescale = track.mdia.mdhd.timescale;
    if timescale == 0 {
        return Err(ProbeError::MissingFrameRate);
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

fn fragments(boxes: &Boxes) -> Result<Vec<Fragment>, ProbeError> {
    let mut byte_offset = boxes
        .sidx_end
        .checked_add(boxes.sidx.first_offset)
        .ok_or(ProbeError::SegmentOffsetOverflow)?;
    let mut fragments = Vec::with_capacity(boxes.sidx.references.len());

    for reference in &boxes.sidx.references {
        let byte_size = u64::from(reference.reference_size);
        let fragment = Fragment::new(
            byte_offset,
            byte_size,
            u64::from(reference.subsegment_duration),
        )
        .ok_or(ProbeError::SegmentRangeOverflow)?;
        fragments.push(fragment);
        byte_offset = byte_offset
            .checked_add(byte_size)
            .ok_or(ProbeError::SegmentOffsetOverflow)?;
    }

    Ok(fragments)
}

fn language(boxes: &Boxes) -> LanguageTag {
    let language = boxes.moov.trak[0].mdia.mdhd.language.as_str();
    language.parse().unwrap_or_else(|_| undetermined_language())
}

fn hevc_codec(prefix: &str, hvcc: &Hvcc) -> String {
    let profile_space = match hvcc.general_profile_space {
        0 => String::new(),
        value => ((b'A' + value - 1) as char).to_string(),
    };
    let compatibility = u32::from_be_bytes(hvcc.general_profile_compatibility_flags).reverse_bits();
    let tier = if hvcc.general_tier_flag { 'H' } else { 'L' };
    let mut codec = format!(
        "{prefix}.{profile_space}{}.{compatibility:x}.{tier}{}",
        hvcc.general_profile_idc, hvcc.general_level_idc
    );

    if let Some(end) = hvcc
        .general_constraint_indicator_flags
        .iter()
        .rposition(|&byte| byte != 0)
    {
        for byte in &hvcc.general_constraint_indicator_flags[..=end] {
            codec.push_str(&format!(".{byte:02x}"));
        }
    }

    codec
}

fn codec_name(codec: &Codec) -> String {
    format!("{codec:?}")
        .split(['(', ' ', '{'])
        .next()
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greatest_common_divisor_reduces_frame_rate_ratio() {
        assert_eq!(greatest_common_divisor(1000, 500), 500);
    }

    #[test]
    fn greatest_common_divisor_handles_coprime_values() {
        assert_eq!(greatest_common_divisor(25, 1), 1);
    }

    #[test]
    fn greatest_common_divisor_handles_equal_values() {
        assert_eq!(greatest_common_divisor(1000, 1000), 1000);
    }

    #[test]
    fn greatest_common_divisor_handles_zero_right_operand() {
        assert_eq!(greatest_common_divisor(1000, 0), 1000);
    }
}
