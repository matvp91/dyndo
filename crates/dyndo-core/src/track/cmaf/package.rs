use std::ops::Range;

use bytes::Bytes;

use super::ResolvedCmafTrack;

/// A temporary in-memory CMAF representation of a source track.
pub struct CmafPackage {
    cmaf: ResolvedCmafTrack,
    bytes: Bytes,
}

impl CmafPackage {
    pub(crate) fn new(cmaf: ResolvedCmafTrack, bytes: Bytes) -> Self {
        Self { cmaf, bytes }
    }

    pub fn cmaf(&self) -> &ResolvedCmafTrack {
        &self.cmaf
    }

    pub fn into_cmaf(self) -> ResolvedCmafTrack {
        self.cmaf
    }

    pub fn read(&self, range: Range<u64>) -> Option<Bytes> {
        let start = usize::try_from(range.start).ok()?;
        let end = usize::try_from(range.end).ok()?;
        (start <= end && end <= self.bytes.len()).then(|| self.bytes.slice(start..end))
    }
}
