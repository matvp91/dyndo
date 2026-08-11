use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use dyndo_core::asset_descriptor::AssetDescriptorError;
use dyndo_core::probe::ProbeError;
use dyndo_core::reader::TrackReadError;
use dyndo_core::thumbnail_track::ThumbnailError;
use dyndo_core::web_vtt_track::WebVttPackageError;
use dyndo_dash::DashError;
use dyndo_hls::HlsError;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("{0}")]
    BadRequest(String),
    #[error("resource not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    AssetDescriptor(#[from] AssetDescriptorError),
    #[error(transparent)]
    Probe(#[from] ProbeError),
    #[error(transparent)]
    WebVttPackage(#[from] WebVttPackageError),
    #[error(transparent)]
    TrackRead(#[from] TrackReadError),
    #[error(transparent)]
    Thumbnail(#[from] ThumbnailError),
    #[error(transparent)]
    Dash(#[from] DashError),
    #[error(transparent)]
    Hls(#[from] HlsError),
    #[error("manifest serialization failed: {0}")]
    Serialization(String),
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let status = self.status();
        (status, self.to_string()).into_response()
    }
}

impl ServerError {
    fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::AssetDescriptor(_)
            | Self::Probe(_)
            | Self::WebVttPackage(_)
            | Self::TrackRead(_)
            | Self::Thumbnail(_)
            | Self::Dash(_)
            | Self::Hls(_)
            | Self::Serialization(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
