use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use dyndo_core::asset::{AssetError, AssetResolveError};
use dyndo_core::track::CmafRepresentationError;
use dyndo_core::track::cmaf::CmafReadError;
use dyndo_core::track::thumbnail::ThumbnailError;
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
    AssetResolve(#[from] AssetResolveError),
    #[error(transparent)]
    CmafRepresentation(#[from] CmafRepresentationError),
    #[error(transparent)]
    CmafRead(#[from] CmafReadError),
    #[error(transparent)]
    Thumbnail(#[from] ThumbnailError),
    #[error(transparent)]
    Dash(#[from] DashError),
    #[error(transparent)]
    Hls(#[from] HlsError),
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
            | Self::AssetResolve(_)
            | Self::CmafRepresentation(_)
            | Self::CmafRead(_)
            | Self::Thumbnail(_)
            | Self::Dash(_)
            | Self::Hls(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
