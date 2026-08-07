//! Decoding the frame shown at a given time.
//!
//! Storage and decoders disagree on how a frame's NAL units are delimited: a CMAF
//! sample prefixes each unit with its length, while a decoder wants them separated
//! by Annex-B start codes and preceded by the parameter sets that describe them.
//! That translation, and the decoder it feeds, live here.
//!
//! A time in the middle of a fragment costs the frames before it. Only the keyframe a
//! fragment opens on decodes on its own, so reaching any later frame means feeding
//! every sample from that keyframe up to it, and discarding all but the last. The
//! `trun` is what says where one sample ends and the next begins, and what time each
//! is shown at.

use std::io::Cursor;
use std::ops::Range;

use mp4_atom::{Atom, Codec, Header, Mdat, Moof, Moov, ReadAtom, ReadFrom};
use openh264::decoder::{DecodeOptions, Flush};
use openh264::formats::YUVSource;

use crate::ThumbnailError;

/// Separates NAL units in a decoder's input, where a length prefix separates them
/// in storage.
const START_CODE: [u8; 4] = [0, 0, 0, 1];

/// The NAL unit type of a coded slice of an IDR picture.
const NAL_IDR: u8 = 5;

/// One decoded picture, as packed 8-bit RGB.
pub(crate) struct Frame {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgb: Vec<u8>,
}

/// One sample of a fragment: where its NAL units sit in the fragment's `mdat`, and
/// the time it is shown at in the track's timescale.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Sample {
    bytes: Range<usize>,
    time: u64,
}

/// A decoder configured by one track's initialization segment: openh264, plus the
/// parameter sets to prefix a keyframe with and the width of the length field the
/// stored NAL units sit behind.
pub(crate) struct AvcDecoder {
    inner: openh264::decoder::Decoder,
    parameters: Vec<u8>,
    length_size: usize,
}

impl AvcDecoder {
    pub(crate) fn new(initialization: &[u8]) -> Result<Self, ThumbnailError> {
        let moov = read_moov(initialization)?;
        let sample_entry = moov
            .trak
            .first()
            .and_then(|track| track.mdia.minf.stbl.stsd.codecs.first())
            .ok_or(ThumbnailError::Container("stsd has no sample entry"))?;
        let Codec::Avc1(entry) = sample_entry else {
            return Err(ThumbnailError::Container("sample entry is not avc1"));
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

    /// Decodes the frame shown at `time` out of the fragment holding it, with `time`
    /// in the track's timescale — the clock the fragment's own timestamps are in.
    ///
    /// The frame shown at an instant is the last one presented at or before it, so a
    /// time between two samples resolves to the earlier of them.
    pub(crate) fn frame_at(&mut self, fragment: &[u8], time: u64) -> Result<Frame, ThumbnailError> {
        let (moof, media) = read_fragment(fragment)?;
        let samples = read_samples(&moof, media.len())?;
        let target = target(&samples, time);

        for (index, sample) in samples[..=target].iter().enumerate() {
            let mut packet = if index == 0 {
                self.parameters.clone()
            } else {
                Vec::new()
            };
            let idr = append_annex_b(&mut packet, &media[sample.bytes.clone()], self.length_size)?;
            // Only a keyframe decodes without the frames before it, and probing
            // rejects a source whose fragments open on anything else.
            if index == 0 && !idr {
                return Err(ThumbnailError::NoKeyframe);
            }

            // Feeding a group of pictures one sample at a time means never flushing
            // between them: openh264 hands back the picture for the sample just fed,
            // and a flush mid-group errors out instead.
            let picture = self.inner.decode_with_options(
                &packet,
                DecodeOptions::new().flush_after_decode(Flush::NoFlush),
            )?;
            if index == target {
                let picture = picture.ok_or(ThumbnailError::EmptyFrame)?;
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

        Err(ThumbnailError::Container("fragment holds no samples"))
    }
}

/// The sample whose picture is on screen at `time`: the last one shown at or before
/// it, or the first sample when `time` precedes them all.
///
/// Samples are searched rather than indexed because presentation order is not decode
/// order — a fragment carrying B-frames stores them out of the order they are shown.
fn target(samples: &[Sample], time: u64) -> usize {
    samples
        .iter()
        .enumerate()
        .filter(|(_, sample)| sample.time <= time)
        .max_by_key(|(_, sample)| sample.time)
        .map_or(0, |(index, _)| index)
}

/// Appends one sample's NAL units to `packet` in Annex-B form, reporting whether the
/// sample holds a coded slice of an IDR picture.
fn append_annex_b(
    packet: &mut Vec<u8>,
    sample: &[u8],
    length_size: usize,
) -> Result<bool, ThumbnailError> {
    let mut offset = 0;
    let mut idr = false;

    while offset + length_size <= sample.len() {
        let length = sample[offset..offset + length_size]
            .iter()
            .fold(0usize, |length, byte| (length << 8) | usize::from(*byte));
        offset += length_size;
        let unit = sample
            .get(offset..offset + length)
            .ok_or(ThumbnailError::Container("nal unit runs past the sample"))?;
        offset += length;

        match unit.first().map(|byte| byte & 0x1f) {
            Some(kind) => idr |= kind == NAL_IDR,
            None => return Err(ThumbnailError::Container("empty nal unit")),
        }
        packet.extend_from_slice(&START_CODE);
        packet.extend_from_slice(unit);
    }

    Ok(idr)
}

/// Walks the top-level boxes of an initialization segment to its `moov`.
fn read_moov(initialization: &[u8]) -> Result<Moov, ThumbnailError> {
    let mut cursor = Cursor::new(initialization);

    loop {
        let header = Header::read_from(&mut cursor)?;
        let size = header
            .size
            .ok_or(ThumbnailError::Container("box has no size"))?;
        if header.kind == Moov::KIND {
            return Ok(Moov::read_atom(&header, &mut cursor)?);
        }
        cursor.set_position(cursor.position() + size as u64);
    }
}

/// A fragment's `moof`, and the `mdat` payload its samples fill.
fn read_fragment(fragment: &[u8]) -> Result<(Moof, &[u8]), ThumbnailError> {
    let mut cursor = Cursor::new(fragment);
    let mut moof = None;

    loop {
        let header = Header::read_from(&mut cursor)?;
        let size = header
            .size
            .ok_or(ThumbnailError::Container("box has no size"))?;
        let start = cursor.position() as usize;
        if header.kind == Moof::KIND {
            moof = Some(Moof::read_atom(&header, &mut cursor)?);
            continue;
        }
        if header.kind == Mdat::KIND {
            let media = fragment
                .get(start..start + size)
                .ok_or(ThumbnailError::Container("mdat runs past the fragment"))?;
            return Ok((
                moof.ok_or(ThumbnailError::Container("fragment has no moof"))?,
                media,
            ));
        }
        cursor.set_position((start + size) as u64);
    }
}

/// The samples a fragment holds, in decode order.
///
/// Sizes and durations come from the `trun` where it carries them and the `tfhd`
/// defaults where it does not, and a sample's time is the fragment's decode time plus
/// the durations before it plus its own composition offset.
fn read_samples(moof: &Moof, media_len: usize) -> Result<Vec<Sample>, ThumbnailError> {
    let traf = moof
        .traf
        .first()
        .ok_or(ThumbnailError::Container("moof has no traf"))?;
    let decode_time = traf
        .tfdt
        .as_ref()
        .map_or(0, |tfdt| tfdt.base_media_decode_time);
    let mut samples = Vec::new();
    let mut offset = 0usize;
    let mut elapsed = 0u64;

    for run in &traf.trun {
        for entry in &run.entries {
            let size = entry
                .size
                .or(traf.tfhd.default_sample_size)
                .ok_or(ThumbnailError::Container("trun entry has no sample size"))?
                as usize;
            let duration = entry.duration.or(traf.tfhd.default_sample_duration).ok_or(
                ThumbnailError::Container("trun entry has no sample duration"),
            )?;
            let end = offset
                .checked_add(size)
                .filter(|end| *end <= media_len)
                .ok_or(ThumbnailError::Container("sample runs past the mdat"))?;
            let time =
                i128::from(decode_time) + i128::from(elapsed) + i128::from(entry.cts.unwrap_or(0));

            samples.push(Sample {
                bytes: offset..end,
                time: u64::try_from(time.max(0)).unwrap_or(u64::MAX),
            });
            offset = end;
            elapsed += u64::from(duration);
        }
    }

    if samples.is_empty() {
        return Err(ThumbnailError::Container("fragment holds no samples"));
    }

    Ok(samples)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use mp4_atom::{Encode, Mfhd, Tfdt, Tfhd, Traf, Trun, TrunEntry};

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
        let Err(error) = AvcDecoder::new(&fixture("video_av1_240.mp4")) else {
            panic!("a track that is not avc unexpectedly configured a decoder");
        };

        assert!(matches!(error, ThumbnailError::Container(_)), "{error}");
    }

    #[test]
    fn samples_run_from_the_fragments_decode_time() {
        let samples = read_samples(&moof(1_000, &[(10, 40, 0), (10, 40, 0)]), 100).unwrap();

        assert_eq!(
            samples.iter().map(|sample| sample.time).collect::<Vec<_>>(),
            vec![1_000, 1_040]
        );
    }

    /// A composition offset is what puts a sample on screen somewhere other than
    /// where its decode time falls, which is how a fragment carries B-frames.
    #[test]
    fn samples_carry_their_composition_offset() {
        let samples = read_samples(&moof(0, &[(10, 40, 80), (10, 40, -40)]), 100).unwrap();

        assert_eq!(
            samples.iter().map(|sample| sample.time).collect::<Vec<_>>(),
            vec![80, 0]
        );
    }

    #[test]
    fn samples_follow_one_another_through_the_mdat() {
        let samples = read_samples(&moof(0, &[(10, 40, 0), (20, 40, 0)]), 100).unwrap();

        assert_eq!(
            samples
                .iter()
                .map(|sample| sample.bytes.clone())
                .collect::<Vec<_>>(),
            vec![0..10, 10..30]
        );
    }

    #[test]
    fn samples_refuse_to_run_past_the_mdat() {
        let error = read_samples(&moof(0, &[(10, 40, 0), (20, 40, 0)]), 15).unwrap_err();

        assert!(matches!(error, ThumbnailError::Container(_)), "{error}");
    }

    #[test]
    fn target_is_the_sample_shown_at_the_time_asked_for() {
        let samples =
            read_samples(&moof(0, &[(10, 40, 0), (10, 40, 0), (10, 40, 0)]), 100).unwrap();

        assert_eq!(target(&samples, 40), 1);
    }

    /// A time inside a sample's own span is still that sample, since it is the one on
    /// screen until the next is presented.
    #[test]
    fn target_holds_a_sample_until_the_next_is_shown() {
        let samples = read_samples(&moof(0, &[(10, 40, 0), (10, 40, 0)]), 100).unwrap();

        assert_eq!(target(&samples, 39), 0);
    }

    /// Presentation order is not decode order, so the sample shown at a time can sit
    /// anywhere in the fragment.
    #[test]
    fn target_searches_presentation_order_rather_than_decode_order() {
        let samples =
            read_samples(&moof(0, &[(10, 40, 80), (10, 40, -40), (10, 40, -40)]), 100).unwrap();

        assert_eq!(
            (
                target(&samples, 0),
                target(&samples, 40),
                target(&samples, 80)
            ),
            (1, 2, 0)
        );
    }

    #[test]
    fn target_falls_back_to_the_first_sample_before_them_all() {
        let samples = read_samples(&moof(1_000, &[(10, 40, 0)]), 100).unwrap();

        assert_eq!(target(&samples, 0), 0);
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

        assert!(matches!(error, ThumbnailError::Container(_)), "{error}");
    }

    #[test]
    fn read_fragment_refuses_one_without_an_mdat() {
        let error = read_fragment(&box_bytes(b"free", &[])).unwrap_err();

        assert!(matches!(error, ThumbnailError::Parse(_)), "{error}");
    }

    fn decoder() -> AvcDecoder {
        AvcDecoder::new(&fixture("video_avc_1080.mp4")).unwrap()
    }

    fn fixture(name: &str) -> Vec<u8> {
        fs::read(format!("{FIXTURES}/{name}")).unwrap()
    }

    /// A `moof` whose single track fragment holds one sample per `(size, duration,
    /// composition offset)` given.
    fn moof(decode_time: u64, samples: &[(u32, u32, i32)]) -> Moof {
        Moof {
            mfhd: Mfhd { sequence_number: 1 },
            traf: vec![Traf {
                tfhd: Tfhd {
                    track_id: 1,
                    ..Default::default()
                },
                tfdt: Some(Tfdt {
                    base_media_decode_time: decode_time,
                }),
                trun: vec![Trun {
                    data_offset: None,
                    entries: samples
                        .iter()
                        .map(|(size, duration, cts)| TrunEntry {
                            duration: Some(*duration),
                            size: Some(*size),
                            flags: None,
                            cts: Some(*cts),
                        })
                        .collect(),
                }],
                ..Default::default()
            }],
        }
    }

    /// One sample's bytes: a length-prefixed NAL unit per `(kind, length)` given.
    /// Only the first byte of each unit carries meaning here, so the rest is zeroed.
    fn sample(units: &[(u8, usize)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for (kind, length) in units {
            bytes.extend_from_slice(&(*length as u32).to_be_bytes());
            bytes.push(*kind);
            bytes.extend(std::iter::repeat_n(0, length - 1));
        }
        bytes
    }

    fn box_bytes(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut bytes = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(body);
        bytes
    }

    /// Kept honest against the encoder: a `moof` built here has to read back as one.
    #[test]
    fn the_test_moof_round_trips_through_the_container() {
        let mut bytes = Vec::new();
        moof(1_000, &[(10, 40, 0)]).encode(&mut bytes).unwrap();
        let fragment = [bytes, box_bytes(b"mdat", &[0; 10])].concat();

        let (moof, media) = read_fragment(&fragment).unwrap();

        assert_eq!(read_samples(&moof, media.len()).unwrap().len(), 1);
    }
}
