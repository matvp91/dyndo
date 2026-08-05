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

#[cfg(test)]
mod tests {
    use opendal::services::Memory;

    use super::*;

    #[tokio::test]
    async fn memory_source_reads_valid_range() {
        let source = source();

        let bytes = source
            .read_range(&operator(), RelativePath::new("unused"), 1..4)
            .await
            .unwrap();

        assert_eq!(bytes, Bytes::from_static(b"bcd"));
    }

    #[tokio::test]
    async fn memory_source_reads_empty_range() {
        let source = source();

        let bytes = source
            .read_range(&operator(), RelativePath::new("unused"), 2..2)
            .await
            .unwrap();

        assert!(bytes.is_empty());
    }

    #[tokio::test]
    async fn memory_source_rejects_reversed_range() {
        let range = Range { start: 4, end: 2 };
        let error = source()
            .read_range(&operator(), RelativePath::new("unused"), range)
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            TrackSourceError::InvalidRange {
                start: 4,
                end: 2,
                length: 6
            }
        ));
    }

    #[tokio::test]
    async fn memory_source_rejects_end_beyond_length() {
        let error = source()
            .read_range(&operator(), RelativePath::new("unused"), 0..7)
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            TrackSourceError::InvalidRange {
                start: 0,
                end: 7,
                length: 6
            }
        ));
    }

    fn source() -> TrackSource {
        TrackSource::Memory {
            bytes: Bytes::from_static(b"abcdef"),
        }
    }

    fn operator() -> Operator {
        Operator::new(Memory::default()).unwrap()
    }
}
