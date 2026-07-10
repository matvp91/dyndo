use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use dyndo_core::CoreError;

#[derive(Debug)]
pub enum ServerError {
    NotFound(String),
    BadRequest(String),
    Internal(String),
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            ServerError::NotFound(m) => (StatusCode::NOT_FOUND, m),
            ServerError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ServerError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
        };
        (status, msg).into_response()
    }
}

/// Map a [`CoreError`] onto an HTTP status. A missing object (OpenDAL
/// `NotFound`) is a 404; every other failure — malformed descriptor JSON,
/// unreadable or unsupported media, other I/O — is a 500, because the asset
/// files are server-owned and a bad one is our problem, not the client's.
impl From<CoreError> for ServerError {
    fn from(e: CoreError) -> Self {
        match &e {
            CoreError::Storage(oe) if oe.kind() == opendal::ErrorKind::NotFound => {
                ServerError::NotFound(e.to_string())
            }
            _ => ServerError::Internal(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_variant_maps_to_404() {
        assert_eq!(
            ServerError::NotFound("x".into()).into_response().status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn bad_request_variant_maps_to_400() {
        assert_eq!(
            ServerError::BadRequest("x".into()).into_response().status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn internal_variant_maps_to_500() {
        assert_eq!(
            ServerError::Internal("x".into()).into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn core_storage_not_found_maps_to_404() {
        let e = CoreError::Storage(opendal::Error::new(
            opendal::ErrorKind::NotFound,
            "missing",
        ));
        assert_eq!(
            ServerError::from(e).into_response().status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn core_container_maps_to_500() {
        let e = CoreError::Container("bad box".into());
        assert_eq!(
            ServerError::from(e).into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn core_unsupported_codec_maps_to_500() {
        let e = CoreError::UnsupportedCodec("video");
        assert_eq!(
            ServerError::from(e).into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
