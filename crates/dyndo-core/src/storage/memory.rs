//! In-memory `Source` used by crate tests.

use crate::error::Result;
use crate::storage::Source;

/// An in-memory `Source` backed by a byte vector.
pub struct BytesSource {
    bytes: Vec<u8>,
}

impl BytesSource {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

impl Source for BytesSource {
    async fn size(&self) -> Result<u64> {
        Ok(self.bytes.len() as u64)
    }

    async fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let start = (offset as usize).min(self.bytes.len());
        let end = (start + len).min(self.bytes.len());
        Ok(self.bytes[start..end].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reads_the_requested_range() {
        let s = BytesSource::new(vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(s.size().await.unwrap(), 6);
        assert_eq!(s.read_at(2, 3).await.unwrap(), vec![2, 3, 4]);
    }

    #[tokio::test]
    async fn read_at_past_end_returns_empty_without_panicking() {
        let s = BytesSource::new(vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(s.read_at(6, 3).await.unwrap(), Vec::<u8>::new());
    }

    #[tokio::test]
    async fn read_at_overhanging_end_returns_partial_bytes() {
        let s = BytesSource::new(vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(s.read_at(4, 10).await.unwrap(), vec![4, 5]);
    }

    #[tokio::test]
    async fn read_at_exactly_at_size_returns_empty() {
        let s = BytesSource::new(vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(s.read_at(6, 0).await.unwrap(), Vec::<u8>::new());
    }

    #[tokio::test]
    async fn read_at_strictly_past_end_returns_empty_without_panicking() {
        // offset (100) is strictly greater than len (6): without the
        // `.min(self.bytes.len())` clamp on `start`, indexing
        // `self.bytes[start..end]` would panic here.
        let s = BytesSource::new(vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(s.read_at(100, 3).await.unwrap(), Vec::<u8>::new());
    }
}
