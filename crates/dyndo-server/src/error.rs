use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use dyndo_core::asset_descriptor::AssetDescriptorError;
use dyndo_core::track::TrackError;
use dyndo_dash::builder::DashError;
use dyndo_hls::builder::HlsError;
use dyndo_text::wvtt::UnpackError;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("invalid options: {0}")]
    InvalidOptions(String),
    #[error("invalid filter: {0}")]
    InvalidFilter(String),
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
    use dyndo_core::filter::FilterMatchedNothing;

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

    #[test]
    fn invalid_filter_maps_to_bad_request() {
        let response = ServerError::InvalidFilter("bad expression".into()).into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    /// A filter that matches nothing is an addressing error, like an unknown track,
    /// even though it reaches the server wrapped in a builder's error.
    #[test]
    fn a_filter_matching_nothing_maps_to_not_found() {
        for error in [
            ServerError::Dash(DashError::Filter(FilterMatchedNothing)),
            ServerError::Hls(HlsError::Filter(FilterMatchedNothing)),
        ] {
            assert_eq!(error.into_response().status(), StatusCode::NOT_FOUND);
        }
    }
}
