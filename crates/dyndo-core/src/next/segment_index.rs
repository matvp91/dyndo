//! A format-independent segment index.

use std::ops::Range;

use opendal::Operator;
use relative_path::RelativePath;

use super::cmaf_header::CmafHeader;
use super::error::Error;
use super::segment::Segment;

/// The segment boundaries on a media timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentIndex {
    /// Byte range containing the track initialization data.
    pub initialization: Range<u64>,
    /// Units per second used by segment start times and durations.
    pub timescale: u32,
    /// Average media bandwidth in bits per second.
    pub bandwidth: u64,
    /// Segments in presentation order.
    pub segments: Vec<Segment>,
}

impl SegmentIndex {
    /// Read the format-independent index of a CMAF track.
    ///
    /// # Errors
    /// Returns an error when the track header cannot be read or indexed.
    pub async fn read(op: &Operator, path: &RelativePath) -> Result<Self, Error> {
        Ok(CmafHeader::read(op, path).await?.segment_index())
    }

    /// Return the segment starting exactly at `start`.
    ///
    /// # Errors
    /// Returns [`Error::SegmentNotFound`] when `start` is not an advertised
    /// segment boundary.
    pub fn segment_at(&self, start: u64) -> Result<Segment, Error> {
        self.segments
            .iter()
            .copied()
            .find(|segment| segment.start == start)
            .ok_or(Error::SegmentNotFound(start))
    }

    /// Return the total duration in this index's timescale.
    pub fn duration(&self) -> u64 {
        self.segments
            .iter()
            .map(|segment| segment.duration)
            .sum()
    }

    /// Return the longest segment duration in this index's timescale.
    pub fn max_segment_duration(&self) -> u64 {
        self.segments
            .iter()
            .map(|segment| segment.duration)
            .max()
            .unwrap_or(0)
    }

    /// Return the first segment's presentation time in this index's timescale.
    pub fn presentation_time_offset(&self) -> u64 {
        self.segments.first().map_or(0, |segment| segment.start)
    }

    /// Return the longest segment duration, rounded up to milliseconds.
    pub fn max_segment_duration_ms(&self) -> u64 {
        self.units_to_milliseconds(self.max_segment_duration())
    }

    /// Return the presentation duration, rounded up to milliseconds.
    pub fn duration_ms(&self) -> u64 {
        self.units_to_milliseconds(self.duration())
    }

    fn units_to_milliseconds(&self, units: u64) -> u64 {
        if self.timescale == 0 {
            return 0;
        }
        let milliseconds = u128::from(units) * 1_000;
        u64::try_from(milliseconds.div_ceil(u128::from(self.timescale))).unwrap_or(u64::MAX)
    }
}
