//! The core error type, [`CoreError`].

/// Anything that can go wrong reading or parsing an asset.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A storage failure from the underlying OpenDAL operator: a missing
    /// object, an I/O error, a permission problem.
    #[error(transparent)]
    Storage(#[from] opendal::Error),
    /// The descriptor JSON could not be (de)serialized.
    #[error("invalid descriptor JSON: {0}")]
    Descriptor(#[from] serde_json::Error),
    /// A track file could not be read or was structurally invalid.
    #[error("malformed track container: {0}")]
    Container(String),
    /// The track file's extension maps to no supported [`Format`](crate::format::Format).
    #[error("unsupported track format: {0}")]
    UnsupportedFormat(String),
}
