//! The crate's error type, [`CoreError`].

/// Anything that can go wrong reading or parsing an asset through `dyndo-core`.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A storage failure from the underlying OpenDAL operator: a missing
    /// object, an I/O error, a permission problem.
    #[error(transparent)]
    Storage(#[from] opendal::Error),
    /// The descriptor JSON could not be (de)serialized.
    #[error("invalid descriptor JSON: {0}")]
    Descriptor(#[from] serde_json::Error),
    /// A CMAF box could not be read or was structurally invalid.
    #[error("malformed CMAF container: {0}")]
    Container(String),
    /// No supported codec was found for the media type (`"video"` / `"audio"`).
    #[error("unsupported {0} codec")]
    UnsupportedCodec(&'static str),
}
