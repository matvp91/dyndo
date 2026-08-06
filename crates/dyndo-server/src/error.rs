use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use dyndo_core::asset_descriptor::AssetDescriptorError;
use dyndo_core::track::TrackError;
use dyndo_dash::builder::DashError;
use dyndo_hls::builder::HlsError;
use dyndo_text::demuxer::UnpackError;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("invalid transport options: {0}")]
    InvalidOptions(String),
    #[error("resource not found: {0}")]
    NotFound(String),
    #[error("segment time overflow for track {0}")]
    SegmentTimeOverflow(String),
    #[error(transparent)]
    AssetDescriptor(#[from] AssetDescriptorError),
    #[error(transparent)]
    Track(#[from] TrackError),
    #[error(transparent)]
    Dash(#[from] DashError),
    #[error(transparent)]
    Hls(#[from] HlsError),
    #[error(transparent)]
    Unpack(#[from] UnpackError),
    #[error("manifest serialization failed: {0}")]
    Serialization(String),
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::InvalidOptions(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::AssetDescriptor(AssetDescriptorError::Storage(error))
                if error.kind() == opendal::ErrorKind::NotFound =>
            {
                StatusCode::NOT_FOUND
            }
            Self::SegmentTimeOverflow(_)
            | Self::AssetDescriptor(_)
            | Self::Track(_)
            | Self::Dash(_)
            | Self::Hls(_)
            | Self::Unpack(_)
            | Self::Serialization(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, self.to_string()).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_options_maps_to_bad_request() {
        let response = ServerError::InvalidOptions("bad rison".into()).into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn missing_resource_maps_to_not_found() {
        let response = ServerError::NotFound("missing".into()).into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
