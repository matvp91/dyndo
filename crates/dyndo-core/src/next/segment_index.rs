//! Format-independent segment timing.

/// A segment's position on a media timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    /// Start time in the containing [`SegmentIndex`]'s timescale.
    pub start: u64,
    /// Duration in the containing [`SegmentIndex`]'s timescale.
    pub duration: u64,
}

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
    /// Returns [`SegmentNotFound`] when `start` is not an advertised segment
    /// boundary.
    pub fn segment_at(&self, start: u64) -> Result<&Segment, SegmentNotFound> {
        self.segments
            .iter()
            .find(|segment| segment.start == start)
            .ok_or(SegmentNotFound { start })
    }
}

/// A requested start time is not present in a segment index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("no segment starts at {start}")]
pub struct SegmentNotFound {
    /// The requested start time.
    pub start: u64,
}
