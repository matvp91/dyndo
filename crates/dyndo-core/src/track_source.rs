use std::ops::Range;

use bytes::Bytes;
use opendal::Operator;
use relative_path::RelativePath;

#[derive(Debug, thiserror::Error)]
pub enum TrackSourceError {
    #[error(transparent)]
    Storage(#[from] opendal::Error),
    #[error("byte range {start}..{end} is outside source length {length}")]
    InvalidRange { start: u64, end: u64, length: u64 },
}

pub(crate) enum TrackSource {
    Stored,
    Memory { bytes: Bytes },
}

impl TrackSource {
    pub(crate) async fn read_range(
        &self,
        op: &Operator,
        path: &RelativePath,
        range: Range<u64>,
    ) -> Result<Bytes, TrackSourceError> {
        match self {
            Self::Stored => Ok(op.read_with(path.as_str()).range(range).await?.to_bytes()),
            Self::Memory { bytes } => {
                let start =
                    usize::try_from(range.start).map_err(|_| TrackSourceError::InvalidRange {
                        start: range.start,
                        end: range.end,
                        length: bytes.len() as u64,
                    })?;
                let end =
                    usize::try_from(range.end).map_err(|_| TrackSourceError::InvalidRange {
                        start: range.start,
                        end: range.end,
                        length: bytes.len() as u64,
                    })?;
                if start > end || end > bytes.len() {
                    return Err(TrackSourceError::InvalidRange {
                        start: range.start,
                        end: range.end,
                        length: bytes.len() as u64,
                    });
                }
                Ok(bytes.slice(start..end))
            }
        }
    }
}
