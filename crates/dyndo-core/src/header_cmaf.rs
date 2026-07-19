//! Reads the header region of a CMAF track into a [`HeaderCmaf`].
//!
//! Box extraction lives in `box_reader` and codec strings in `codec`:
//! [`HeaderCmaf::read`] scans the file into a `Boxes`, and
//! `build_segments` / `first_sample_duration` / [`rfc6381`] fold the
//! scanned boxes into the stored values.

use mp4_atom::{Moof, Moov, Sidx};
use opendal::Operator;

use crate::box_reader;
use crate::codec::rfc6381;
use crate::error::CoreError;
use crate::segment::Segment;

/// The values dyndo keeps from a CMAF track's header region (`moov`, `sidx`,
/// and the first `moof`). The boxes themselves are folded into these fields
/// at read time and dropped; `mdat` is never fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderCmaf {
    /// Units per second for durations in this track. Never zero: the box
    /// scan rejects a zero `sidx` timescale, so consumers divide freely.
    pub timescale: u32,
    /// Presentation time of the first (sub)segment, in the track timescale.
    pub earliest_presentation_time: u64,
    /// Byte offset just past the `moov` box; the init segment is `0..moov_end`.
    pub moov_end: u64,
    /// Duration of the track's first sample, in the track timescale. `0`
    /// when the file doesn't declare one.
    pub sample_duration: u32,
    /// RFC 6381 codecs parameter of the track's sample entry
    /// (e.g. `"avc1.640028"`).
    pub codec: String,
    /// The track's (sub)segments, in presentation order.
    pub segments: Vec<Segment>,
}

impl HeaderCmaf {
    /// Read the header region of the CMAF track at `path` through `op` and
    /// fold it into a [`HeaderCmaf`].
    ///
    /// # Errors
    /// [`CoreError::Storage`]/[`CoreError::Io`] if the object cannot be
    /// read; [`CoreError::Parse`]/[`CoreError::Container`] if a box cannot
    /// be decoded or a required box is missing or empty;
    /// [`CoreError::UnsupportedCodec`] on a sample entry dyndo does not
    /// support.
    pub async fn read(op: &Operator, path: &str) -> Result<HeaderCmaf, CoreError> {
        let boxes = box_reader::scan(op, path).await?;
        Ok(HeaderCmaf {
            timescale: boxes.sidx.timescale,
            earliest_presentation_time: boxes.sidx.earliest_presentation_time,
            moov_end: boxes.moov_end,
            sample_duration: first_sample_duration(&boxes.moof, &boxes.moov),
            codec: rfc6381(&boxes.moov.trak[0].mdia.minf.stbl.stsd.codecs[0])?,
            segments: build_segments(&boxes.sidx, boxes.sidx_end),
        })
    }

    /// Total presentation duration, in the track timescale.
    pub fn duration(&self) -> u64 {
        self.segments.iter().map(|s| s.duration).sum()
    }

    /// Average bitrate in bits/s, derived from the segment sizes and
    /// duration. `0` when the track has no duration.
    pub fn bandwidth(&self) -> u32 {
        let duration = self.duration();
        if duration == 0 {
            return 0;
        }
        let bytes: u64 = self.segments.iter().map(|s| s.size).sum();
        let seconds = duration as f64 / self.timescale as f64;
        (bytes as f64 * 8.0 / seconds).round() as u32
    }

    /// Frame rate as a (numerator, denominator) ratio, in frames per second.
    /// `(0, 1)` when the track declares no sample duration.
    pub fn frame_rate(&self) -> (u32, u32) {
        if self.sample_duration == 0 {
            return (0, 1);
        }
        let g = gcd(self.timescale, self.sample_duration);
        (self.timescale / g, self.sample_duration / g)
    }

    /// Location of the init segment (`ftyp`+`moov`) within the track file.
    pub fn init_segment(&self) -> Segment {
        Segment {
            offset: 0,
            size: self.moov_end,
            duration: 0,
        }
    }
}

/// Fold the `sidx` into the segment map: each reference's size and duration,
/// with byte offsets accumulated from the end of the `sidx` box.
fn build_segments(sidx: &Sidx, sidx_end: u64) -> Vec<Segment> {
    let mut offset = sidx_end + sidx.first_offset;
    let mut out = Vec::with_capacity(sidx.references.len());
    for r in &sidx.references {
        out.push(Segment {
            offset,
            size: r.reference_size as u64,
            duration: r.subsegment_duration as u64,
        });
        offset += r.reference_size as u64;
    }
    out
}

/// The duration of the track's first sample: the first `trun` entry's
/// duration, else the `tfhd` default, else the `trex` default, else `0`.
fn first_sample_duration(moof: &Moof, moov: &Moov) -> u32 {
    let from_traf = moof.traf.first().and_then(|traf| {
        traf.trun
            .iter()
            .flat_map(|t| &t.entries)
            .find_map(|e| e.duration)
            .or(traf.tfhd.default_sample_duration)
    });
    from_traf
        .or_else(|| {
            moov.mvex
                .as_ref()
                .and_then(|m| m.trex.first())
                .map(|t| t.default_sample_duration)
        })
        .unwrap_or(0)
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}
