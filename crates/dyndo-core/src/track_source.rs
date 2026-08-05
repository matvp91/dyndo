use std::ops::Range;

use bytes::Bytes;
use opendal::Operator;
use relative_path::RelativePath;

#[derive(Debug, thiserror::Error)]
pub enum TrackSourceError {
    #[error(transparent)]
    Storage(#[from] opendal::Error),
    #[error("invalid byte range {0:?}")]
    InvalidRange(Range<u64>),
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
                // `Bytes::slice` panics on reversed and out-of-bounds ranges.
                if range.start > range.end || range.end > bytes.len() as u64 {
                    return Err(TrackSourceError::InvalidRange(range));
                }

                // Both bounds are within the length, so narrowing back is lossless.
                Ok(bytes.slice(range.start as usize..range.end as usize))
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
        assert_eq!(read(1..4).await.unwrap(), Bytes::from_static(b"bcd"));
    }

    #[tokio::test]
    async fn memory_source_reads_empty_range() {
        assert!(read(2..2).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn memory_source_rejects_reversed_range() {
        let error = read(Range { start: 4, end: 2 }).await.unwrap_err();

        assert_eq!(error.to_string(), "invalid byte range 4..2");
    }

    #[tokio::test]
    async fn memory_source_rejects_end_beyond_length() {
        let error = read(0..7).await.unwrap_err();

        assert_eq!(error.to_string(), "invalid byte range 0..7");
    }

    async fn read(range: Range<u64>) -> Result<Bytes, TrackSourceError> {
        let source = TrackSource::Memory {
            bytes: Bytes::from_static(b"abcdef"),
        };

        source
            .read_range(
                &Operator::new(Memory::default()).unwrap(),
                RelativePath::new("unused"),
                range,
            )
            .await
    }
}
