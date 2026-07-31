//! Format-independent segment timing and grouping.

mod group;
mod index;

pub use index::SegmentIndex;

/// A segment's position on a media timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    /// Start time in the containing [`SegmentIndex`]'s timescale.
    pub start: u64,
    /// Duration in the containing [`SegmentIndex`]'s timescale.
    pub duration: u64,
}

impl Segment {
    /// End time in the containing [`SegmentIndex`]'s timescale.
    pub fn end(self) -> u64 {
        self.start + self.duration
    }
}
