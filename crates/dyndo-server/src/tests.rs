use std::path::PathBuf;

use axum::body::{to_bytes, Body};
use axum::http::{header::CONTENT_TYPE, Request, StatusCode};
use serde_json::json;
use tower::ServiceExt; // for `oneshot`

use crate::routes::build_router;
use crate::state::AppState;

/// Path to a committed header-only fixture in dyndo-core.
fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("dyndo-core/tests/fixtures")
        .join(name)
}

/// Create `{tmp}/test/asset.json` + copy the given fixture files into that dir.
/// Returns the tempdir (keep it alive) and an AppState rooted at `{tmp}`.
fn setup(asset_json: serde_json::Value, files: &[&str]) -> (tempfile::TempDir, AppState) {
    let tmp = tempfile::tempdir().unwrap();
    let asset_dir = tmp.path().join("test");
    std::fs::create_dir_all(&asset_dir).unwrap();
    std::fs::write(
        asset_dir.join("asset.json"),
        serde_json::to_vec(&asset_json).unwrap(),
    )
    .unwrap();
    for f in files {
        std::fs::copy(fixture_path(f), asset_dir.join(f)).unwrap();
    }
    let state = AppState::new(tmp.path().to_path_buf());
    (tmp, state)
}

fn video_and_audio_asset() -> serde_json::Value {
    json!({"tracks": [
        {"type": "video", "id": "v0", "source": "video_avc_1080.mp4",
         "fourcc": "avc1", "timescale": 90000, "width": 1920, "height": 1080},
        {"type": "audio", "id": "a0", "source": "audio_aac_nl_2.mp4",
         "fourcc": "mp4a", "timescale": 48000, "sample_rate": 48000, "channels": 2, "language": "nld"}
    ]})
}

#[tokio::test]
async fn serves_manifest() {
    let (_tmp, state) = setup(
        video_and_audio_asset(),
        &["video_avc_1080.mp4", "audio_aac_nl_2.mp4"],
    );
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test/dash/index.mpd")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()[CONTENT_TYPE], "application/dash+xml");
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let xml = String::from_utf8(body.to_vec()).unwrap();
    assert!(xml.contains("<MPD"));
    assert!(xml.contains("$RepresentationID$/$Time$.m4s"));
    assert!(xml.contains("codecs=\"avc1.640028\""));
    assert!(xml.contains("codecs=\"mp4a.40.2\""));
}

#[tokio::test]
async fn manifest_sets_cors_header() {
    let (_tmp, state) = setup(
        video_and_audio_asset(),
        &["video_avc_1080.mp4", "audio_aac_nl_2.mp4"],
    );
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test/dash/index.mpd")
                .header("origin", "http://example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(resp.headers().contains_key("access-control-allow-origin"));
}

#[tokio::test]
async fn unknown_asset_is_404() {
    let tmp = tempfile::tempdir().unwrap();
    let app = build_router(AppState::new(tmp.path().to_path_buf()));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/nope/dash/index.mpd")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
