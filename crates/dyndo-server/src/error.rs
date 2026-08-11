use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use dyndo_core::asset::AssetDescriptorError;
use dyndo_core::probe::ProbeError;
use dyndo_core::reader::TrackReadError;
use dyndo_core::track::synthetic::SyntheticTrackError;
use dyndo_core::track::timed_text::web_vtt::TimedTextPackageError;
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
    TimedTextPackage(#[from] TimedTextPackageError),
    #[error(transparent)]
    TrackRead(#[from] TrackReadError),
    #[error(transparent)]
    SyntheticTrack(#[from] SyntheticTrackError),
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
            | Self::TimedTextPackage(_)
            | Self::TrackRead(_)
            | Self::SyntheticTrack(_)
            | Self::Dash(_)
            | Self::Hls(_)
            | Self::Serialization(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
