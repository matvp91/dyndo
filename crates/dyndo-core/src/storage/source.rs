//! The backend-agnostic byte source. Its primitive is a ranged read.

use crate::error::Result;

/// A range-addressable blob of bytes: a local file, an S3 object, …
#[allow(async_fn_in_trait)]
pub trait Source {
    /// Total size in bytes (stat / HEAD).
    async fn size(&self) -> Result<u64>;
    /// Read `len` bytes starting at `offset` (pread / ranged GET). May return
    /// fewer bytes only at end-of-source.
    async fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>>;
}
