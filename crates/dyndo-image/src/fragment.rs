//! The samples one CMAF fragment holds.
//!
//! Which bytes are a frame, and what time it is shown at, is a question about the
//! container rather than the codec: the `trun` says where one sample ends and the
//! next begins, and a sample's time is the fragment's decode time plus the durations
//! before it plus its own composition offset. Every decoder needs that same answer,
//! so none of them has to work it out.

use std::io::Cursor;
use std::ops::Range;

use mp4_atom::{Atom, Header, Mdat, Moof, Moov, ReadAtom, ReadFrom};

#[derive(Debug, thiserror::Error)]
pub enum FragmentError {
    #[error("malformed track container: {0}")]
    Parse(#[from] mp4_atom::Error),
    #[error("invalid track container: {0}")]
    Container(&'static str),
}

/// One sample: where its bytes sit in the fragment's `mdat`, and the time it is shown
/// at in the track's timescale.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Sample {
    bytes: Range<usize>,
    time: u64,
}

/// A fragment's samples, over the bytes they point into.
#[derive(Debug)]
pub struct Fragment<'a> {
    media: &'a [u8],
    samples: Vec<Sample>,
}

impl<'a> Fragment<'a> {
    /// Reads the `moof` and `mdat` of one fragment into the samples it holds, in
    /// decode order.
    ///
    /// # Errors
    ///
    /// Returns a [`FragmentError`] when the fragment is malformed, or declares a
    /// sample whose size or duration nothing supplies.
    pub fn read(bytes: &'a [u8]) -> Result<Self, FragmentError> {
        let (moof, media) = split(bytes)?;
        let traf = moof
            .traf
            .first()
            .ok_or(FragmentError::Container("moof has no traf"))?;
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
                    .ok_or(FragmentError::Container("trun entry has no sample size"))?
                    as usize;
                let duration = entry.duration.or(traf.tfhd.default_sample_duration).ok_or(
                    FragmentError::Container("trun entry has no sample duration"),
                )?;
                let end = offset
                    .checked_add(size)
                    .filter(|end| *end <= media.len())
                    .ok_or(FragmentError::Container("sample runs past the mdat"))?;
                let time = i128::from(decode_time)
                    + i128::from(elapsed)
                    + i128::from(entry.cts.unwrap_or(0));

                samples.push(Sample {
                    bytes: offset..end,
                    time: u64::try_from(time.max(0)).unwrap_or(u64::MAX),
                });
                offset = end;
                elapsed += u64::from(duration);
            }
        }

        if samples.is_empty() {
            return Err(FragmentError::Container("fragment holds no samples"));
        }

        Ok(Self { media, samples })
    }

    /// The sample whose picture is on screen at `time`: the last one shown at or
    /// before it, or the first when `time` precedes them all.
    ///
    /// Samples are searched rather than indexed because presentation order is not
    /// decode order — a fragment carrying B-frames stores them out of the order they
    /// are shown.
    pub fn shown_at(&self, time: u64) -> usize {
        self.samples
            .iter()
            .enumerate()
            .filter(|(_, sample)| sample.time <= time)
            .max_by_key(|(_, sample)| sample.time)
            .map_or(0, |(index, _)| index)
    }

    /// The bytes of every sample from the one the fragment opens on up to and
    /// including `index`, which is what reaching a frame in the middle of a fragment
    /// costs.
    pub fn upto(&self, index: usize) -> impl Iterator<Item = &[u8]> {
        let last = index.min(self.samples.len() - 1);

        self.samples[..=last]
            .iter()
            .map(|sample| &self.media[sample.bytes.clone()])
    }
}

/// A fragment's `moof`, and the `mdat` payload its samples fill.
fn split(bytes: &[u8]) -> Result<(Moof, &[u8]), FragmentError> {
    let mut cursor = Cursor::new(bytes);
    let mut moof = None;

    loop {
        let header = Header::read_from(&mut cursor)?;
        let size = header
            .size
            .ok_or(FragmentError::Container("box has no size"))?;
        let start = cursor.position() as usize;
        if header.kind == Moof::KIND {
            moof = Some(Moof::read_atom(&header, &mut cursor)?);
            continue;
        }
        if header.kind == Mdat::KIND {
            let media = bytes
                .get(start..start + size)
                .ok_or(FragmentError::Container("mdat runs past the fragment"))?;
            return Ok((
                moof.ok_or(FragmentError::Container("fragment has no moof"))?,
                media,
            ));
        }
        cursor.set_position((start + size) as u64);
    }
}

/// Walks the top-level boxes of an initialization segment to its `moov`, which is
/// where a decoder finds how the track it is about to read was coded.
///
/// # Errors
///
/// Returns a [`FragmentError`] when the segment is malformed or holds no `moov`.
pub fn read_moov(initialization: &[u8]) -> Result<Moov, FragmentError> {
    let mut cursor = Cursor::new(initialization);

    loop {
        let header = Header::read_from(&mut cursor)?;
        let size = header
            .size
            .ok_or(FragmentError::Container("box has no size"))?;
        if header.kind == Moov::KIND {
            return Ok(Moov::read_atom(&header, &mut cursor)?);
        }
        cursor.set_position(cursor.position() + size as u64);
    }
}

#[cfg(test)]
mod tests {
    use mp4_atom::{Encode, Mfhd, Tfdt, Tfhd, Traf, Trun, TrunEntry};

    use super::*;

    #[test]
    fn samples_run_from_the_fragments_decode_time() {
        let fragment = fragment(1_000, &[(10, 40, 0), (10, 40, 0)]);

        let times = Fragment::read(&fragment)
            .unwrap()
            .samples
            .iter()
            .map(|sample| sample.time)
            .collect::<Vec<_>>();

        assert_eq!(times, vec![1_000, 1_040]);
    }

    /// A composition offset is what puts a sample on screen somewhere other than
    /// where its decode time falls, which is how a fragment carries B-frames.
    #[test]
    fn samples_carry_their_composition_offset() {
        let fragment = fragment(0, &[(10, 40, 80), (10, 40, -40)]);

        let times = Fragment::read(&fragment)
            .unwrap()
            .samples
            .iter()
            .map(|sample| sample.time)
            .collect::<Vec<_>>();

        assert_eq!(times, vec![80, 0]);
    }

    #[test]
    fn samples_follow_one_another_through_the_mdat() {
        let fragment = fragment(0, &[(10, 40, 0), (20, 40, 0)]);

        let bytes = Fragment::read(&fragment)
            .unwrap()
            .samples
            .iter()
            .map(|sample| sample.bytes.clone())
            .collect::<Vec<_>>();

        assert_eq!(bytes, vec![0..10, 10..30]);
    }

    #[test]
    fn read_refuses_a_sample_running_past_the_mdat() {
        let mut bytes = Vec::new();
        moof(0, &[(10, 40, 0), (20, 40, 0)])
            .encode(&mut bytes)
            .unwrap();
        let fragment = [bytes, box_bytes(b"mdat", &[0; 15])].concat();

        let error = Fragment::read(&fragment).unwrap_err();

        assert!(matches!(error, FragmentError::Container(_)), "{error}");
    }

    #[test]
    fn read_refuses_a_fragment_without_an_mdat() {
        let error = Fragment::read(&box_bytes(b"free", &[])).unwrap_err();

        assert!(matches!(error, FragmentError::Parse(_)), "{error}");
    }

    #[test]
    fn shown_at_is_the_sample_on_screen_at_the_time_asked_for() {
        let fragment = fragment(0, &[(10, 40, 0), (10, 40, 0), (10, 40, 0)]);

        assert_eq!(Fragment::read(&fragment).unwrap().shown_at(40), 1);
    }

    /// A time inside a sample's own span is still that sample, since it is the one on
    /// screen until the next is presented.
    #[test]
    fn shown_at_holds_a_sample_until_the_next_is_shown() {
        let fragment = fragment(0, &[(10, 40, 0), (10, 40, 0)]);

        assert_eq!(Fragment::read(&fragment).unwrap().shown_at(39), 0);
    }

    /// Presentation order is not decode order, so the sample shown at a time can sit
    /// anywhere in the fragment.
    #[test]
    fn shown_at_searches_presentation_order_rather_than_decode_order() {
        let fragment = fragment(0, &[(10, 40, 80), (10, 40, -40), (10, 40, -40)]);
        let fragment = Fragment::read(&fragment).unwrap();

        assert_eq!(
            (
                fragment.shown_at(0),
                fragment.shown_at(40),
                fragment.shown_at(80)
            ),
            (1, 2, 0)
        );
    }

    #[test]
    fn shown_at_falls_back_to_the_first_sample_before_them_all() {
        let fragment = fragment(1_000, &[(10, 40, 0)]);

        assert_eq!(Fragment::read(&fragment).unwrap().shown_at(0), 0);
    }

    /// Reaching a frame costs the samples before it, so the ones handed over run from
    /// the fragment's first up to the one asked for.
    #[test]
    fn upto_yields_every_sample_from_the_first() {
        let fragment = fragment(0, &[(10, 40, 0), (20, 40, 0), (30, 40, 0)]);
        let fragment = Fragment::read(&fragment).unwrap();

        assert_eq!(
            fragment.upto(1).map(<[u8]>::len).collect::<Vec<_>>(),
            vec![10, 20]
        );
    }

    /// A `moof`/`mdat` pair holding one sample per `(size, duration, composition
    /// offset)` given, encoded through the container so the reader is held honest.
    fn fragment(decode_time: u64, samples: &[(u32, u32, i32)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        moof(decode_time, samples).encode(&mut bytes).unwrap();
        let media = samples.iter().map(|(size, _, _)| *size as usize).sum();

        [bytes, box_bytes(b"mdat", &vec![0; media])].concat()
    }

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

    fn box_bytes(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut bytes = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(body);
        bytes
    }
}
