use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use dyndo_core::asset_descriptor::AssetDescriptorError;
use dyndo_core::probe::ProbeError;
use dyndo_core::reader::TrackReadError;
use dyndo_core::text::wvtt::WvttParseError;
use dyndo_dash::DashError;
use dyndo_hls::HlsError;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("invalid options: {0}")]
    InvalidOptions(String),
    #[error("invalid filter: {0}")]
    InvalidFilter(String),
    #[error("resource not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    AssetDescriptor(#[from] AssetDescriptorError),
    #[error(transparent)]
    Probe(#[from] ProbeError),
    #[error(transparent)]
    TrackRead(#[from] TrackReadError),
    #[error(transparent)]
    Dash(#[from] DashError),
    #[error(transparent)]
    Hls(#[from] HlsError),
    #[error(transparent)]
    WvttParse(#[from] WvttParseError),
    #[error("manifest serialization failed: {0}")]
    Serialization(String),
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::InvalidOptions(_) | Self::InvalidFilter(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            // A filter that narrowed an asset down to nothing is an addressing error,
            // like an unknown track id, rather than a fault in the asset.
            Self::Dash(DashError::Filter(_)) | Self::Hls(HlsError::Filter(_)) => {
                StatusCode::NOT_FOUND
            }
            Self::AssetDescriptor(AssetDescriptorError::Storage(error))
                if error.kind() == opendal::ErrorKind::NotFound =>
            {
                StatusCode::NOT_FOUND
            }
            Self::AssetDescriptor(_)
            | Self::Probe(_)
            | Self::TrackRead(_)
            | Self::Dash(_)
            | Self::Hls(_)
            | Self::WvttParse(_)
            | Self::Serialization(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, self.to_string()).into_response()
    }
}
