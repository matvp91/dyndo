//! Route table. All DASH routes live under `/{asset}/dash/`.

mod dash;

use std::sync::Arc;

use axum::{routing::get, Router};
use tower_http::cors::{Any, CorsLayer};

use crate::config::Config;

pub(crate) fn build_router(config: Arc<Config>) -> Router {
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any);
    Router::new()
        .route("/{asset}/dash/index.mpd", get(dash::manifest))
        .route("/{asset}/dash/{repr}/init.mp4", get(dash::init_segment))
        .route("/{asset}/dash/{repr}/{seg}", get(dash::media_segment))
        .with_state(config)
        .layer(cors)
}

#[cfg(test)]
mod tests {
    use axum::body::{to_bytes, Body};
    use axum::http::{header, Request, StatusCode};
    use tower::ServiceExt;

    use super::*;
    use crate::config::Config;

    const FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../dyndo-core/tests/fixtures/video_avc_1080.mp4"
    );

    // A base dir containing one asset `clip` with a single video representation `v0`.
    fn app() -> (tempfile::TempDir, Router) {
        let base = tempfile::tempdir().unwrap();
        let asset_dir = base.path().join("clip");
        std::fs::create_dir(&asset_dir).unwrap();
        std::fs::copy(FIXTURE, asset_dir.join("video.mp4")).unwrap();
        std::fs::write(
            asset_dir.join("asset.json"),
            r#"{"tracks":[{"type":"video","id":"v0","source":"video.mp4",
                "fourcc":"avc1","timescale":90000,"width":1920,"height":1080}]}"#,
        )
        .unwrap();
        let config = Arc::new(Config {
            assets_base_path: base.path().to_path_buf(),
            port: 0,
        });
        (base, build_router(config))
    }

    async fn get(router: Router, uri: &str) -> (StatusCode, Option<String>, Vec<u8>) {
        let resp = router
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let content_type = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|v| v.to_str().unwrap().to_owned());
        let body = to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec();
        (status, content_type, body)
    }

    #[tokio::test]
    async fn init_segment_is_served_as_the_ftyp_led_init() {
        let (_base, router) = app();
        let (status, ct, body) = get(router, "/clip/dash/v0/init.mp4").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(ct.as_deref(), Some("video/mp4"));
        assert_eq!(&body[4..8], b"ftyp");
    }

    #[tokio::test]
    async fn media_segment_at_time_zero_is_served_as_a_moof() {
        let (_base, router) = app();
        let (status, ct, body) = get(router, "/clip/dash/v0/0.m4s").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(ct.as_deref(), Some("video/mp4"));
        assert_eq!(&body[4..8], b"moof");
    }

    #[tokio::test]
    async fn missing_segment_time_is_404() {
        let (_base, router) = app();
        assert_eq!(get(router, "/clip/dash/v0/999.m4s").await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn non_m4s_name_is_404() {
        let (_base, router) = app();
        assert_eq!(get(router, "/clip/dash/v0/cover.jpg").await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn non_numeric_time_is_400() {
        let (_base, router) = app();
        assert_eq!(get(router, "/clip/dash/v0/abc.m4s").await.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unknown_representation_is_404() {
        let (_base, router) = app();
        assert_eq!(get(router, "/clip/dash/nope/init.mp4").await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn unknown_asset_is_404() {
        let (_base, router) = app();
        assert_eq!(get(router, "/nope/dash/v0/init.mp4").await.0, StatusCode::NOT_FOUND);
    }
}
