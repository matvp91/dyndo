//! AVC, by way of openh264.
//!
//! Storage and decoders disagree on how a frame's NAL units are delimited: a CMAF
//! sample prefixes each unit with its length, while a decoder wants them separated by
//! Annex-B start codes and preceded by the parameter sets that describe them. That
//! translation is all this adds to the fragment it is handed.

use mp4_atom::Codec;
use openh264::decoder::{DecodeOptions, Flush};
use openh264::formats::YUVSource;

use super::{Decode, DecodeError, Frame};
use crate::fragment::{self, Fragment};

/// The sample entry this decoder handles.
pub const SAMPLE_ENTRY: &str = "avc1";

/// Separates NAL units in a decoder's input, where a length prefix separates them in
/// storage.
const START_CODE: [u8; 4] = [0, 0, 0, 1];

/// The NAL unit type of a coded slice of an IDR picture.
const NAL_IDR: u8 = 5;

/// openh264, plus the parameter sets to prefix a keyframe with and the width of the
/// length field the stored NAL units sit behind.
pub struct Decoder {
    inner: openh264::decoder::Decoder,
    parameters: Vec<u8>,
    length_size: usize,
}

impl Decoder {
    /// Configures a decoder from a track's initialization segment.
    ///
    /// # Errors
    ///
    /// Returns a [`DecodeError`] when the segment is malformed or describes a track
    /// that is not AVC.
    pub fn new(initialization: &[u8]) -> Result<Self, DecodeError> {
        let moov = fragment::read_moov(initialization)?;
        let sample_entry = moov
            .trak
            .first()
            .and_then(|track| track.mdia.minf.stbl.stsd.codecs.first())
            .ok_or(DecodeError::Stream("stsd has no sample entry"))?;
        let Codec::Avc1(entry) = sample_entry else {
            return Err(DecodeError::Stream("sample entry is not avc1"));
        };

        let mut parameters = Vec::new();
        for set in entry
            .avcc
            .sequence_parameter_sets
            .iter()
            .chain(&entry.avcc.picture_parameter_sets)
        {
            parameters.extend_from_slice(&START_CODE);
            parameters.extend_from_slice(set);
        }

        Ok(Self {
            inner: openh264::decoder::Decoder::new()?,
            parameters,
            length_size: usize::from(entry.avcc.length_size),
        })
    }
}

impl Decode for Decoder {
    fn frame_at(&mut self, fragment: &Fragment<'_>, time: u64) -> Result<Frame, DecodeError> {
        let target = fragment.shown_at(time);

        for (index, sample) in fragment.upto(target).enumerate() {
            let mut packet = if index == 0 {
                self.parameters.clone()
            } else {
                Vec::new()
            };
            let idr = append_annex_b(&mut packet, sample, self.length_size)?;
            // Only a keyframe decodes without the frames before it, and probing
            // rejects a source whose fragments open on anything else.
            if index == 0 && !idr {
                return Err(DecodeError::Stream("fragment does not open on a keyframe"));
            }

            // openh264 hands back the picture for the sample just fed, and flushing
            // between samples of one group errors out — but the last sample has to be
            // flushed, or a target that is its own group's keyframe holds its picture
            // back and nothing comes out at all.
            let flush = if index == target {
                Flush::Flush
            } else {
                Flush::NoFlush
            };
            let picture = self
                .inner
                .decode_with_options(&packet, DecodeOptions::new().flush_after_decode(flush))?;
            if index == target {
                let picture = picture.ok_or(DecodeError::EmptyFrame)?;
                let (width, height) = picture.dimensions();
                let mut rgb = vec![0u8; width * height * 3];
                picture.write_rgb8(&mut rgb);

                return Ok(Frame {
                    width: width as u32,
                    height: height as u32,
                    rgb,
                });
            }
        }

        Err(DecodeError::EmptyFrame)
    }
}

/// Appends one sample's NAL units to `packet` in Annex-B form, reporting whether the
/// sample holds a coded slice of an IDR picture.
fn append_annex_b(
    packet: &mut Vec<u8>,
    sample: &[u8],
    length_size: usize,
) -> Result<bool, DecodeError> {
    let mut offset = 0;
    let mut idr = false;

    while offset + length_size <= sample.len() {
        let length = sample[offset..offset + length_size]
            .iter()
            .fold(0usize, |length, byte| (length << 8) | usize::from(*byte));
        offset += length_size;
        let unit = sample
            .get(offset..offset + length)
            .ok_or(DecodeError::Stream("nal unit runs past the sample"))?;
        offset += length;

        match unit.first().map(|byte| byte & 0x1f) {
            Some(kind) => idr |= kind == NAL_IDR,
            None => return Err(DecodeError::Stream("empty nal unit")),
        }
        packet.extend_from_slice(&START_CODE);
        packet.extend_from_slice(unit);
    }

    Ok(idr)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");

    #[test]
    fn new_takes_the_parameter_sets_from_an_avc_initialization_segment() {
        let decoder = decoder();

        assert!(decoder.parameters.starts_with(&START_CODE) && decoder.length_size == 4);
    }

    /// Both parameter sets are carried, so the second start code opens the picture
    /// parameter set rather than the prefix ending after the sequence one.
    #[test]
    fn new_carries_both_parameter_sets() {
        let decoder = decoder();

        assert_eq!(
            decoder
                .parameters
                .windows(START_CODE.len())
                .filter(|window| *window == START_CODE)
                .count(),
            2
        );
    }

    #[test]
    fn new_refuses_a_track_that_is_not_avc() {
        let Err(error) = Decoder::new(&fixture("video_av1_240.mp4")) else {
            panic!("a track that is not avc unexpectedly configured a decoder");
        };

        assert!(matches!(error, DecodeError::Stream(_)), "{error}");
    }

    #[test]
    fn append_annex_b_replaces_each_length_prefix_with_a_start_code() {
        let mut packet = Vec::new();

        let idr = append_annex_b(&mut packet, &sample(&[(NAL_IDR, 3), (1, 2)]), 4).unwrap();

        assert_eq!(
            (packet, idr),
            (
                [&START_CODE[..], &[NAL_IDR, 0, 0], &START_CODE[..], &[1, 0]].concat(),
                true
            )
        );
    }

    #[test]
    fn append_annex_b_reports_a_sample_holding_no_idr() {
        let mut packet = Vec::new();

        let idr = append_annex_b(&mut packet, &sample(&[(1, 3)]), 4).unwrap();

        assert!(!idr);
    }

    #[test]
    fn append_annex_b_refuses_a_unit_whose_length_runs_past_the_sample() {
        let mut packet = Vec::new();
        let sample = [&u32::MAX.to_be_bytes()[..], &[NAL_IDR, 0, 0]].concat();

        let error = append_annex_b(&mut packet, &sample, 4).unwrap_err();

        assert!(matches!(error, DecodeError::Stream(_)), "{error}");
    }

    fn decoder() -> Decoder {
        Decoder::new(&fixture("video_avc_1080.mp4")).unwrap()
    }

    fn fixture(name: &str) -> Vec<u8> {
        fs::read(format!("{FIXTURES}/{name}")).unwrap()
    }

    /// One sample's bytes: a length-prefixed NAL unit per `(kind, length)` given. Only
    /// the first byte of each unit carries meaning here, so the rest is zeroed.
    fn sample(units: &[(u8, usize)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for (kind, length) in units {
            bytes.extend_from_slice(&(*length as u32).to_be_bytes());
            bytes.push(*kind);
            bytes.extend(std::iter::repeat_n(0, length - 1));
        }
        bytes
    }
}
