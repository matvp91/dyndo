//! A (sub)segment's location within a track file.

use std::ops::Range;

/// A (sub)segment's location: byte `offset`/`size` plus `duration` in the
/// track timescale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    /// Byte offset of this (sub)segment within the track file.
    pub offset: u64,
    /// Size of this (sub)segment, in bytes.
    pub size: u64,
    /// Duration of this (sub)segment, in the track timescale.
    pub duration: u64,
}

impl Segment {
    /// Byte offset just past this (sub)segment.
    pub fn end(&self) -> u64 {
        self.offset + self.size
    }

    /// The byte range of this (sub)segment within the track file.
    pub fn range(&self) -> Range<u64> {
        self.offset..self.end()
    }
}
