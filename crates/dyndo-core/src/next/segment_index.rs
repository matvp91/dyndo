//! A format-independent segment index.

use super::error::Error;
use super::segment::Segment;

/// The segment boundaries on a media timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentIndex {
    /// Units per second used by segment start times and durations.
    pub timescale: u32,
    /// Segments in presentation order.
    pub segments: Vec<Segment>,
}

impl SegmentIndex {
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
}
