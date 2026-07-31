//! On-demand probing of a CMAF track's initialization and segment index.

use std::ops::Range;

use mp4_atom::Sidx;
use opendal::Operator;
use relative_path::RelativePath;

use super::box_reader;
use super::error::Error;
use super::segment::Segment;
use super::segment_index::SegmentIndex;

/// The initialization range and indexed fragments of a CMAF track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmafHeader {
    /// Byte range containing the track initialization data.
    pub initialization: Range<u64>,
    /// Units per second used by segment start times and durations.
    pub timescale: u32,
    /// Media fragments in presentation order.
    pub fragments: Vec<Fragment>,
}

/// A CMAF fragment's timing and location in its source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    /// Start time in the header's timescale.
    pub start: u64,
    /// Duration in the header's timescale.
    pub duration: u64,
    /// The fragment's byte range in the source file.
    pub range: Range<u64>,
}

impl Fragment {
    fn end(&self) -> u64 {
        self.start + self.duration
    }
}

impl CmafHeader {
    /// Probe the CMAF header at `path` without reading media payloads.
    ///
    /// # Errors
    /// Returns an error when storage cannot be read, required MP4 boxes are
    /// invalid or absent, or the `sidx` uses hierarchical references.
    pub async fn read(op: &Operator, path: &RelativePath) -> Result<Self, Error> {
        let boxes = box_reader::scan(op, path).await?;
        let fragments = fragments_from_sidx(&boxes.sidx, boxes.sidx_end, path)?;

        Ok(Self {
            initialization: 0..boxes.moov_end,
            timescale: boxes.sidx.timescale,
            fragments,
        })
    }

    /// Resolve a segment timing interval to its CMAF source byte range.
    ///
    /// # Errors
    /// Returns [`Error::RangeNotFound`] unless both ends of `segment`
    /// coincide with raw CMAF segment boundaries.
    pub fn byte_range(&self, segment: Segment) -> Result<Range<u64>, Error> {
        let target_end = segment
            .start
            .checked_add(segment.duration)
            .ok_or_else(|| range_not_found(segment))?;
        let start = self
            .fragments
            .iter()
            .position(|fragment| fragment.start == segment.start)
            .ok_or_else(|| range_not_found(segment))?;
        let range_start = self.fragments[start].range.start;

        for fragment in &self.fragments[start..] {
            if fragment.end() == target_end {
                return Ok(range_start..fragment.range.end);
            }
            if fragment.end() > target_end {
                break;
            }
        }

        Err(range_not_found(segment))
    }

    /// Return the format-independent timing index for this track.
    pub fn segment_index(&self) -> SegmentIndex {
        SegmentIndex {
            initialization: self.initialization.clone(),
            timescale: self.timescale,
            bandwidth: self.bandwidth(),
            segments: self
                .fragments
                .iter()
                .map(|fragment| Segment {
                    start: fragment.start,
                    duration: fragment.duration,
                })
                .collect(),
        }
    }

    fn duration(&self) -> u64 {
        self.fragments
            .iter()
            .map(|fragment| fragment.duration)
            .sum()
    }

    /// Longest fragment duration, rounded up to milliseconds.
    pub fn max_fragment_duration_ms(&self) -> u64 {
        self.fragments
            .iter()
            .map(|fragment| units_to_milliseconds(fragment.duration, self.timescale))
            .max()
            .unwrap_or(0)
    }

    /// Average media bandwidth in bits per second.
    fn bandwidth(&self) -> u64 {
        let duration = u128::from(self.duration());
        if duration == 0 {
            return 0;
        }
        let bytes: u128 = self
            .fragments
            .iter()
            .map(|fragment| u128::from(fragment.range.end - fragment.range.start))
            .sum();
        let bits_per_second = bytes * 8 * u128::from(self.timescale) / duration;
        u64::try_from(bits_per_second).unwrap_or(u64::MAX)
    }
}

fn units_to_milliseconds(units: u64, timescale: u32) -> u64 {
    let milliseconds = u128::from(units) * 1_000;
    u64::try_from(milliseconds.div_ceil(u128::from(timescale))).unwrap_or(u64::MAX)
}

fn fragments_from_sidx(
    sidx: &Sidx,
    sidx_end: u64,
    path: &RelativePath,
) -> Result<Vec<Fragment>, Error> {
    let mut offset = checked_add(sidx_end, sidx.first_offset, path)?;
    let mut start = sidx.earliest_presentation_time;
    let mut fragments = Vec::with_capacity(sidx.references.len());

    for reference in &sidx.references {
        if reference.reference_type {
            return Err(invalid_track(
                path,
                "hierarchical segment indexes are unsupported",
            ));
        }

        let size = u64::from(reference.reference_size);
        let duration = u64::from(reference.subsegment_duration);
        let end = checked_add(offset, size, path)?;

        fragments.push(Fragment {
            start,
            duration,
            range: offset..end,
        });

        offset = end;
        start = checked_add(start, duration, path)?;
    }

    Ok(fragments)
}

fn checked_add(left: u64, right: u64, path: &RelativePath) -> Result<u64, Error> {
    left.checked_add(right)
        .ok_or_else(|| invalid_track(path, "segment-index timing or byte offset overflows"))
}

fn invalid_track(path: &RelativePath, reason: &str) -> Error {
    Error::InvalidTrack {
        path: path.to_owned(),
        reason: reason.to_string(),
    }
}

fn range_not_found(segment: Segment) -> Error {
    Error::RangeNotFound(segment)
}
