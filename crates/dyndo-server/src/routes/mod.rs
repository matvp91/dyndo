//! Route table and protocol dispatch. Every stream lives under
//! `/{asset_path}/{protocol}/…`, where `{asset_path}` is a (possibly nested)
//! descriptor JSON path relative to the assets base and `{protocol}` is one of
//! [`PROTOCOLS`]. The descriptor path is variable-length and sits *before* the
//! fixed protocol infix, which no declarative route can express — so the whole
//! tail is captured by one catch-all and split by [`split_route`].

mod dash;
mod serve;

use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    response::Response,
    routing::get,
    Router,
};
use tower_http::cors::{Any, CorsLayer};

use crate::error::ServerError;

/// Streaming protocols we recognise. The manifest resource differs per protocol
/// (`index.mpd` for DASH); media segments are the same CMAF for all. To light up
/// HLS, add `"hls"` here and a sibling manifest arm in [`dispatch`].
const PROTOCOLS: &[&str] = &["dash"];

pub(crate) fn build_router(assets_base: Arc<PathBuf>) -> Router {
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any);
    Router::new()
        .route("/{*path}", get(dispatch))
        .with_state(assets_base)
        .layer(cors)
}

/// Route the catch-all tail to the right handler.
async fn dispatch(
    State(assets_base): State<Arc<PathBuf>>,
    Path(path): Path<String>,
) -> Result<Response, ServerError> {
    let path = path.strip_prefix('/').unwrap_or(path.as_str());
    let (asset_path, protocol, resource) = split_route(path)
        .ok_or_else(|| ServerError::NotFound(format!("not a streaming route: {path}")))?;

    // A resource with a slash is `{repr}/…` — a media segment, served straight
    // from CMAF and identical for every protocol. A bare resource is the
    // manifest, which each protocol renders its own way.
    match resource.split_once('/') {
        Some((repr, "init.mp4")) => serve::init_segment(&assets_base, asset_path, repr).await,
        Some((repr, seg)) => serve::media_segment(&assets_base, asset_path, repr, seg).await,
        None => match (protocol, resource) {
            ("dash", "index.mpd") => dash::manifest(&assets_base, asset_path).await,
            _ => Err(ServerError::NotFound(format!(
                "no {protocol} resource {resource}"
            ))),
        },
    }
}

/// Split `{asset_path}/{protocol}/{resource}` on the *rightmost* `/{protocol}/`
/// for a known protocol, so a descriptor directory named after a protocol still
/// resolves. Returns `None` when no known protocol delimiter is present.
fn split_route(path: &str) -> Option<(&str, &str, &str)> {
    let mut best: Option<(usize, &str, usize)> = None;
    for proto in PROTOCOLS.iter().copied() {
        let delim = format!("/{proto}/");
        if let Some(i) = path.rfind(&delim) {
            if best.is_none_or(|(bi, _, _)| i > bi) {
                best = Some((i, proto, i + delim.len()));
            }
        }
    }
    best.map(|(i, proto, res_start)| (&path[..i], proto, &path[res_start..]))
}

#[cfg(test)]
mod tests {
    use std::path::Path as StdPath;

    use axum::body::{to_bytes, Body};
    use axum::http::{header, Request, StatusCode};
    use tower::ServiceExt;

    use super::*;

    const FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../dyndo-core/tests/fixtures/video_avc_1080.mp4"
    );

    const ASSET_JSON: &str = r#"{"tracks":[{"type":"video","id":"v0","source":"video.mp4",
        "fourcc":"avc1","timescale":90000,"width":1920,"height":1080}]}"#;

    // A base dir with a descriptor `asset.json` (+ its `video.mp4` source) at the
    // root and a second copy nested under `movies/clip/`, exercising both flat and
    // nested descriptor paths through the same representation `v0`.
    fn app() -> (tempfile::TempDir, Router) {
        let base = tempfile::tempdir().unwrap();
        write_asset(base.path());
        let nested = base.path().join("movies/clip");
        std::fs::create_dir_all(&nested).unwrap();
        write_asset(&nested);
        let assets_base = Arc::new(base.path().to_path_buf());
        (base, build_router(assets_base))
    }

    fn write_asset(dir: &StdPath) {
        std::fs::copy(FIXTURE, dir.join("video.mp4")).unwrap();
        std::fs::write(dir.join("asset.json"), ASSET_JSON).unwrap();
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
        let (status, ct, body) = get(router, "/asset.json/dash/v0/init.mp4").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(ct.as_deref(), Some("video/mp4"));
        assert_eq!(&body[4..8], b"ftyp");
    }

    #[tokio::test]
    async fn media_segment_at_time_zero_is_served_as_a_moof() {
        let (_base, router) = app();
        let (status, ct, body) = get(router, "/asset.json/dash/v0/0.m4s").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(ct.as_deref(), Some("video/mp4"));
        assert_eq!(&body[4..8], b"moof");
    }

    #[tokio::test]
    async fn nested_descriptor_manifest_is_served() {
        let (_base, router) = app();
        let (status, ct, _) = get(router, "/movies/clip/asset.json/dash/index.mpd").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(ct.as_deref(), Some("application/dash+xml"));
    }

    #[tokio::test]
    async fn nested_descriptor_segment_is_served() {
        let (_base, router) = app();
        let (status, _, body) = get(router, "/movies/clip/asset.json/dash/v0/init.mp4").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(&body[4..8], b"ftyp");
    }

    #[tokio::test]
    async fn missing_segment_time_is_404() {
        let (_base, router) = app();
        assert_eq!(get(router, "/asset.json/dash/v0/999.m4s").await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn non_m4s_name_is_404() {
        let (_base, router) = app();
        assert_eq!(get(router, "/asset.json/dash/v0/cover.jpg").await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn non_numeric_time_is_400() {
        let (_base, router) = app();
        assert_eq!(get(router, "/asset.json/dash/v0/abc.m4s").await.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unknown_representation_is_404() {
        let (_base, router) = app();
        assert_eq!(get(router, "/asset.json/dash/nope/init.mp4").await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn unknown_asset_is_404() {
        let (_base, router) = app();
        assert_eq!(get(router, "/nope.json/dash/v0/init.mp4").await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn unknown_protocol_is_404() {
        let (_base, router) = app();
        assert_eq!(get(router, "/asset.json/hls/index.m3u8").await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn path_without_a_protocol_is_404() {
        let (_base, router) = app();
        assert_eq!(get(router, "/asset.json").await.0, StatusCode::NOT_FOUND);
    }

    #[test]
    fn split_route_separates_asset_protocol_and_resource() {
        assert_eq!(
            split_route("movies/clip/asset.json/dash/index.mpd"),
            Some(("movies/clip/asset.json", "dash", "index.mpd"))
        );
        assert_eq!(
            split_route("asset.json/dash/v0/0.m4s"),
            Some(("asset.json", "dash", "v0/0.m4s"))
        );
    }

    #[test]
    fn split_route_uses_the_rightmost_protocol_delimiter() {
        // A descriptor directory literally named `dash` stays in the asset path.
        assert_eq!(
            split_route("dash/asset.json/dash/index.mpd"),
            Some(("dash/asset.json", "dash", "index.mpd"))
        );
    }

    #[test]
    fn split_route_is_none_without_a_known_protocol() {
        assert_eq!(split_route("asset.json/hls/index.m3u8"), None);
        assert_eq!(split_route("asset.json"), None);
    }
}
