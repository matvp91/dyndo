use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use dyndo_core::asset::AssetError;
use dyndo_core::track::SourceResolveError;
use dyndo_core::track::cmaf::CmafReadError;
use dyndo_core::track::thumbnail::ThumbnailError;
use dyndo_core::track::timed_text::WebVttPackageError;
use dyndo_dash::DashError;
use dyndo_hls::HlsError;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("{0}")]
    BadRequest(String),
    #[error("resource not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    Asset(#[from] AssetError),
    #[error(transparent)]
    SourceResolve(#[from] SourceResolveError),
    #[error(transparent)]
    WebVttPackage(#[from] WebVttPackageError),
    #[error(transparent)]
    CmafRead(#[from] CmafReadError),
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
            Self::Asset(_)
            | Self::SourceResolve(_)
            | Self::WebVttPackage(_)
            | Self::CmafRead(_)
            | Self::Thumbnail(_)
            | Self::Dash(_)
            | Self::Hls(_)
            | Self::Serialization(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
