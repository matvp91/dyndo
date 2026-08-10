//! What a served segment's bytes hold: the frames inside them.
//!
//! Which bytes are a frame, and what time it is shown at, is a question about the
//! container rather than the codec: the `trun` says where one frame ends and the next
//! begins, and a frame's time is its fragment's decode time plus the durations before it
//! plus its own composition offset. Every decoder needs that same answer, so none of
//! them has to work it out.
//!
//! A segment groups fragments, so its bytes are one `moof`/`mdat` pair or several in a
//! row, and the frames of each follow the ones before it. Nothing here reads: the bytes
//! were fetched to be served, and the frames in them come for free.

use std::io::Cursor;

use mp4_atom::{Atom, Header, Mdat, Moof, ReadAtom, ReadFrom};

#[derive(Debug, thiserror::Error)]
pub enum FrameReaderError {
    #[error("malformed track container: {0}")]
    Parse(#[from] mp4_atom::Error),
    #[error("invalid track container: {0}")]
    Container(&'static str),
}

/// One frame: its bytes, the time it is shown at in the track's timescale, and whether
/// it is the one its fragment opens on.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Frame<'a> {
    bytes: &'a [u8],
    time: u64,
    opens: bool,
}

/// A segment's frames, over the bytes they point into.
#[derive(Debug)]
pub struct FrameReader<'a> {
    frames: Vec<Frame<'a>>,
}

impl<'a> FrameReader<'a> {
    /// Reads every `moof`/`mdat` pair in `segment` into the frames it holds, in decode
    /// order.
    ///
    /// # Errors
    ///
    /// Returns a [`FrameReaderError`] when the segment is malformed, or declares a frame
    /// whose size or duration nothing supplies.
    pub fn read(segment: &'a [u8]) -> Result<Self, FrameReaderError> {
        let mut cursor = Cursor::new(segment);
        let mut frames = Vec::new();
        let mut moof: Option<Moof> = None;

        while (cursor.position() as usize) < segment.len() {
            let header = Header::read_from(&mut cursor)?;
            let size = header
                .size
                .ok_or(FrameReaderError::Container("box has no size"))?;
            let start = cursor.position() as usize;

            if header.kind == Moof::KIND {
                moof = Some(Moof::read_atom(&header, &mut cursor)?);
            } else if header.kind == Mdat::KIND {
                let moof = moof
                    .take()
                    .ok_or(FrameReaderError::Container("mdat has no moof"))?;
                let media = start
                    .checked_add(size)
                    .and_then(|end| segment.get(start..end))
                    .ok_or(FrameReaderError::Container("mdat runs past the segment"))?;
                read_fragment(&moof, media, &mut frames)?;
            }
            cursor.set_position((start + size) as u64);
        }

        if frames.is_empty() {
            return Err(FrameReaderError::Container("segment holds no frames"));
        }

        Ok(Self { frames })
    }

    /// The frame on screen at `time`: the last one shown at or before it, or the first
    /// when `time` precedes them all.
    ///
    /// Frames are searched rather than indexed because presentation order is not decode
    /// order — a fragment carrying B-frames stores them out of the order they are shown.
    pub fn shown_at(&self, time: u64) -> usize {
        self.frames
            .iter()
            .enumerate()
            .filter(|(_, frame)| frame.time <= time)
            .max_by_key(|(_, frame)| frame.time)
            .map_or(0, |(index, _)| index)
    }

    /// The bytes of every frame from the one its fragment opens on up to and including
    /// `index`, which is what reaching a frame in the middle of a fragment costs.
    ///
    /// A fragment opens on a keyframe, so a grouped segment costs no more than the one
    /// fragment the frame is in.
    pub fn upto(&self, index: usize) -> impl ExactSizeIterator<Item = &'a [u8]> {
        let last = index.min(self.frames.len() - 1);
        let first = self.frames[..=last]
            .iter()
            .rposition(|frame| frame.opens)
            .unwrap_or(0);

        self.frames[first..=last].iter().map(|frame| frame.bytes)
    }
}

/// Reads one fragment's frames into `frames`, over the `mdat` payload they fill.
fn read_fragment<'a>(
    moof: &Moof,
    media: &'a [u8],
    frames: &mut Vec<Frame<'a>>,
) -> Result<(), FrameReaderError> {
    let traf = moof
        .traf
        .first()
        .ok_or(FrameReaderError::Container("moof has no traf"))?;
    let decode_time = traf
        .tfdt
        .as_ref()
        .map_or(0, |tfdt| tfdt.base_media_decode_time);
    let mut offset = 0usize;
    let mut elapsed = 0u64;
    let mut opens = true;

    for run in &traf.trun {
        for entry in &run.entries {
            let size = entry
                .size
                .or(traf.tfhd.default_sample_size)
                .ok_or(FrameReaderError::Container("trun entry has no frame size"))?
                as usize;
            let duration = entry.duration.or(traf.tfhd.default_sample_duration).ok_or(
                FrameReaderError::Container("trun entry has no frame duration"),
            )?;
            let end = offset
                .checked_add(size)
                .filter(|end| *end <= media.len())
                .ok_or(FrameReaderError::Container("frame runs past the mdat"))?;
            let time =
                i128::from(decode_time) + i128::from(elapsed) + i128::from(entry.cts.unwrap_or(0));

            frames.push(Frame {
                bytes: &media[offset..end],
                time: u64::try_from(time.max(0)).unwrap_or(u64::MAX),
                opens,
            });
            offset = end;
            elapsed += u64::from(duration);
            opens = false;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use mp4_atom::{Encode, Mfhd, Tfdt, Tfhd, Traf, Trun, TrunEntry};

    use super::*;

    #[test]
    fn frames_run_from_their_fragments_decode_time() {
        let segment = fragment(1_000, &[(10, 40, 0), (10, 40, 0)]);

        let times = times(&segment);

        assert_eq!(times, vec![1_000, 1_040]);
    }

    /// A composition offset is what puts a frame on screen somewhere other than where
    /// its decode time falls, which is how a fragment carries B-frames.
    #[test]
    fn frames_carry_their_composition_offset() {
        let segment = fragment(0, &[(10, 40, 80), (10, 40, -40)]);

        let times = times(&segment);

        assert_eq!(times, vec![80, 0]);
    }

    #[test]
    fn frames_follow_one_another_through_the_mdat() {
        let segment = fragment(0, &[(10, 40, 0), (20, 40, 0)]);

        let bytes = bytes(&segment);

        // The mdat counts up from zero, so the byte a frame opens on is the offset it
        // begins at: the second frame starts where the first of ten bytes ended.
        assert_eq!(
            (bytes[0].len(), bytes[0][0], bytes[1].len(), bytes[1][0]),
            (10, 0, 20, 10)
        );
    }

    /// A segment holding several fragments is read as one run of frames, each pointing
    /// into the `mdat` of the fragment it came from.
    #[test]
    fn frames_continue_through_every_fragment_of_a_segment() {
        let segment = [
            fragment(0, &[(10, 40, 0), (10, 40, 0)]),
            fragment(80, &[(10, 40, 0)]),
        ]
        .concat();

        let bytes = bytes(&segment);

        // Each mdat counts up from zero of its own, so the third frame opening on zero
        // is it pointing into the second fragment's rather than running on through the
        // first's.
        assert_eq!(
            (times(&segment), bytes[1][0], bytes[2][0], bytes[2].len()),
            (vec![0, 40, 80], 10, 0, 10)
        );
    }

    #[test]
    fn read_refuses_a_frame_running_past_the_mdat() {
        let mut bytes = Vec::new();
        moof(0, &[(10, 40, 0), (20, 40, 0)])
            .encode(&mut bytes)
            .unwrap();
        let segment = [bytes, box_bytes(b"mdat", &[0; 15])].concat();

        let error = FrameReader::read(&segment).unwrap_err();

        assert!(matches!(error, FrameReaderError::Container(_)), "{error}");
    }

    #[test]
    fn read_refuses_a_segment_without_an_mdat() {
        let error = FrameReader::read(&box_bytes(b"free", &[])).unwrap_err();

        assert!(matches!(error, FrameReaderError::Container(_)), "{error}");
    }

    #[test]
    fn shown_at_is_the_frame_on_screen_at_the_time_asked_for() {
        let segment = fragment(0, &[(10, 40, 0), (10, 40, 0), (10, 40, 0)]);

        assert_eq!(FrameReader::read(&segment).unwrap().shown_at(40), 1);
    }

    /// A time inside a frame's own span is still that frame, since it is the one on
    /// screen until the next is presented.
    #[test]
    fn shown_at_holds_a_frame_until_the_next_is_shown() {
        let segment = fragment(0, &[(10, 40, 0), (10, 40, 0)]);

        assert_eq!(FrameReader::read(&segment).unwrap().shown_at(39), 0);
    }

    /// Presentation order is not decode order, so the frame shown at a time can sit
    /// anywhere in the segment.
    #[test]
    fn shown_at_searches_presentation_order_rather_than_decode_order() {
        let segment = fragment(0, &[(10, 40, 80), (10, 40, -40), (10, 40, -40)]);
        let frames = FrameReader::read(&segment).unwrap();

        assert_eq!(
            (frames.shown_at(0), frames.shown_at(40), frames.shown_at(80)),
            (1, 2, 0)
        );
    }

    #[test]
    fn shown_at_falls_back_to_the_first_frame_before_them_all() {
        let segment = fragment(1_000, &[(10, 40, 0)]);

        assert_eq!(FrameReader::read(&segment).unwrap().shown_at(0), 0);
    }

    /// Reaching a frame costs the frames before it, so the ones handed over run from its
    /// fragment's first up to the one asked for.
    #[test]
    fn upto_yields_every_frame_from_the_first() {
        let segment = fragment(0, &[(10, 40, 0), (20, 40, 0), (30, 40, 0)]);
        let frames = FrameReader::read(&segment).unwrap();

        assert_eq!(
            frames.upto(1).map(<[u8]>::len).collect::<Vec<_>>(),
            vec![10, 20]
        );
    }

    /// Every fragment opens on a keyframe, so a frame in the second one is reached
    /// without decoding the first.
    #[test]
    fn upto_starts_at_the_fragment_the_frame_is_in() {
        let segment = [
            fragment(0, &[(10, 40, 0), (10, 40, 0)]),
            fragment(80, &[(20, 40, 0), (30, 40, 0)]),
        ]
        .concat();
        let frames = FrameReader::read(&segment).unwrap();

        assert_eq!(
            frames.upto(3).map(<[u8]>::len).collect::<Vec<_>>(),
            vec![20, 30]
        );
    }

    fn times(segment: &[u8]) -> Vec<u64> {
        FrameReader::read(segment)
            .unwrap()
            .frames
            .iter()
            .map(|frame| frame.time)
            .collect()
    }

    fn bytes(segment: &[u8]) -> Vec<&[u8]> {
        FrameReader::read(segment)
            .unwrap()
            .frames
            .iter()
            .map(|frame| frame.bytes)
            .collect()
    }

    /// A `moof`/`mdat` pair holding one frame per `(size, duration, composition offset)`
    /// given, encoded through the container so the reader is held honest.
    ///
    /// The `mdat` counts up from zero, so where a frame's bytes begin can be read off
    /// the byte it opens on.
    fn fragment(decode_time: u64, frames: &[(u32, u32, i32)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        moof(decode_time, frames).encode(&mut bytes).unwrap();
        let media: usize = frames.iter().map(|(size, _, _)| *size as usize).sum();
        let media: Vec<u8> = (0..media).map(|byte| byte as u8).collect();

        [bytes, box_bytes(b"mdat", &media)].concat()
    }

    fn moof(decode_time: u64, frames: &[(u32, u32, i32)]) -> Moof {
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
                    entries: frames
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
