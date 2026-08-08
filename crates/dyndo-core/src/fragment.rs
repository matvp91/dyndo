//! What a source stores: one fragment of a CMAF track.
//!
//! A fragment is read off the segment index rather than measured, so nothing here
//! parses media. It is the unit segments are cut from, and the only one dyndo does not
//! choose — the source decided where these edges fall when it was encoded.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fragment {
    pub(crate) byte_offset: u64,
    pub(crate) byte_size: u64,
    pub(crate) raw_duration: u32,
}

impl Fragment {
    /// A fragment of `byte_size` bytes at `byte_offset`, lasting `raw_duration`
    /// timescale units.
    ///
    /// Returns `None` when the range it names runs past the end of addressable
    /// storage, which a segment index claiming an impossible size would ask for.
    pub(crate) fn new(byte_offset: u64, byte_size: u64, raw_duration: u32) -> Option<Self> {
        byte_offset.checked_add(byte_size)?;
        Some(Self {
            byte_offset,
            byte_size,
            raw_duration,
        })
    }

    pub(crate) fn byte_range(&self) -> Range<u64> {
        self.byte_offset..self.byte_offset + self.byte_size
    }

    /// Returns the fragment's duration in the track's timescale units.
    pub(crate) fn raw_duration(&self) -> u32 {
        self.raw_duration
    }
}
