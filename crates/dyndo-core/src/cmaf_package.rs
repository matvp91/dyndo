use std::ops::Range;

use bytes::Bytes;

use super::cmaf_track::CmafTrack;

/// A temporary in-memory CMAF representation of a source track.
pub struct CmafPackage {
    cmaf: CmafTrack,
    bytes: Bytes,
}

impl CmafPackage {
    pub(crate) fn new(cmaf: CmafTrack, bytes: Bytes) -> Self {
        Self { cmaf, bytes }
    }

    pub fn cmaf(&self) -> &CmafTrack {
        &self.cmaf
    }

    pub fn into_cmaf(self) -> CmafTrack {
        self.cmaf
    }

    pub fn read(&self, range: Range<u64>) -> Option<Bytes> {
        let start = usize::try_from(range.start).ok()?;
        let end = usize::try_from(range.end).ok()?;
        (start <= end && end <= self.bytes.len()).then(|| self.bytes.slice(start..end))
    }
}
