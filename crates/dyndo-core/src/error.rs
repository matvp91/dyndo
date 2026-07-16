//! The crate's error type, [`CoreError`].

use crate::codec::MediaType;

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
    /// The descriptor parsed as JSON but its content is invalid (e.g.
    /// unsorted `segment_boundaries`).
    #[error("invalid descriptor: {0}")]
    InvalidDescriptor(String),
    /// A CMAF box could not be read or was structurally invalid.
    #[error("malformed CMAF container: {0}")]
    Container(String),
    /// No supported codec was found for the track's [`MediaType`].
    #[error("unsupported {0} codec")]
    UnsupportedCodec(MediaType),
}
