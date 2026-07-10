use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

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

/// Map the boxed errors returned by `dyndo_core` reads onto HTTP statuses: a
/// missing descriptor (OpenDAL `NotFound`) is a 404, anything else — malformed
/// JSON, other I/O — a 500. CMAF parse/read failures panic in the core by design
/// and never surface here.
impl From<Box<dyn std::error::Error>> for ServerError {
    fn from(e: Box<dyn std::error::Error>) -> Self {
        match e.downcast_ref::<opendal::Error>() {
            Some(oe) if oe.kind() == opendal::ErrorKind::NotFound => {
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
    fn variants_map_to_expected_status() {
        assert_eq!(
            ServerError::NotFound("x".into()).into_response().status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            ServerError::BadRequest("x".into()).into_response().status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ServerError::Internal("x".into()).into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn opendal_not_found_maps_to_404() {
        let e: Box<dyn std::error::Error> =
            Box::new(opendal::Error::new(opendal::ErrorKind::NotFound, "missing"));
        assert_eq!(
            ServerError::from(e).into_response().status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn other_error_maps_to_500() {
        let e: Box<dyn std::error::Error> =
            Box::new(opendal::Error::new(opendal::ErrorKind::Unexpected, "boom"));
        assert_eq!(
            ServerError::from(e).into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
