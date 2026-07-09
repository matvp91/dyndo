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

impl From<dyndo_core::Error> for ServerError {
    fn from(e: dyndo_core::Error) -> Self {
        use dyndo_core::Error;
        match &e {
            Error::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
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
    fn missing_file_maps_to_not_found() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "nope");
        let err: ServerError = dyndo_core::Error::Io {
            path: "a.mp4".into(),
            source: io,
        }
        .into();
        assert_eq!(err.into_response().status(), StatusCode::NOT_FOUND);
    }
}
