//! Reads a raw (non-CMAF) track file whole into a [`HeaderRaw`].

use bytes::Bytes;
use opendal::Operator;

use crate::error::CoreError;

/// The raw contents of a non-CMAF track file (e.g. a plain `.vtt`): such
/// formats have no separable header region, so the whole file is held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderRaw {
    /// The file's bytes.
    pub bytes: Bytes,
}

impl HeaderRaw {
    /// Read the whole file at `path` through `op`.
    ///
    /// # Errors
    /// [`CoreError::Storage`] if the object cannot be read.
    pub async fn read(op: &Operator, path: &str) -> Result<HeaderRaw, CoreError> {
        let buf = op.read(path).await?;
        Ok(HeaderRaw {
            bytes: buf.to_bytes(),
        })
    }
}
