//! The core error type, [`CoreError`].

/// Anything that can go wrong reading or parsing an asset.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A storage failure from the underlying OpenDAL operator: a missing
    /// object, an I/O error, a permission problem.
    #[error(transparent)]
    Storage(#[from] opendal::Error),
    /// The track byte stream failed mid-read.
    #[error("track read failed: {0}")]
    Io(#[from] std::io::Error),
    /// The descriptor JSON could not be (de)serialized.
    #[error("invalid descriptor JSON: {0}")]
    Descriptor(#[from] serde_json::Error),
    /// A track file's box structure could not be decoded.
    #[error("malformed track container: {0}")]
    Parse(#[from] mp4_atom::Error),
    /// A track file decoded, but violates a structural requirement: a
    /// required box is missing or empty.
    #[error("invalid track container: {0}")]
    Container(String),
    /// The track's sample entry declares a codec dyndo does not support.
    #[error("unsupported codec: {0}")]
    UnsupportedCodec(String),
    /// The track file's extension maps to no supported [`Format`](crate::format::Format).
    #[error("unsupported track format: {0}")]
    UnsupportedFormat(String),
}
