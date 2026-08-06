mod context;
mod segment;
mod transport;

use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
    routing::get,
};
use dyndo_dash::options::DashOptions;
use dyndo_hls::options::HlsOptions;
use opendal::Operator;
use tower_http::cors::{Any, CorsLayer};

use crate::error::ServerError;
use crate::routes::context::parse_context;

pub(crate) fn build_router(op: Operator) -> Router {
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any);
    Router::new()
        .route("/health", get(health))
        .route("/out/{options}/{*resource}", get(dispatch))
        .with_state(op)
        .layer(cors)
}

async fn health() -> StatusCode {
    StatusCode::OK
}

async fn dispatch(
    State(op): State<Operator>,
    Path((options, resource)): Path<(String, String)>,
) -> Result<Response, ServerError> {
    match resource.as_str() {
        // Generate the asset's DASH manifest.
        "index.mpd" => {
            let context = parse_context::<DashOptions>(&options)?;
            transport::dash_manifest(&op, &context).await
        }
        // Generate the HLS multivariant playlist.
        "master.m3u8" => {
            let context = parse_context::<HlsOptions>(&options)?;
            transport::hls_master(&op, &context).await
        }
        // Generate the media playlist for the track named by the filename.
        resource if !resource.contains('/') && resource.ends_with(".m3u8") => {
            let track_id = resource
                .strip_suffix(".m3u8")
                .ok_or_else(|| ServerError::NotFound(resource.to_string()))?;
            let context = parse_context::<HlsOptions>(&options)?;
            transport::hls_media(&op, &context, track_id).await
        }
        // Serve initialization bytes, a WebVTT document, or media bytes for the
        // track named by the path.
        resource => {
            let context = parse_context::<()>(&options)?;
            let (track_id, file) = resource
                .split_once('/')
                .ok_or_else(|| ServerError::NotFound(resource.to_string()))?;
            if file == "init.mp4" {
                segment::initialization(&op, &context, track_id).await
            } else if file.ends_with(".vtt") {
                segment::text(&op, &context, track_id, file).await
            } else {
                segment::media(&op, &context, track_id, file).await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use opendal::services::Fs;
    use tempfile::TempDir;
    use tower::ServiceExt;

    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");

    #[tokio::test]
    async fn health_route_returns_ok() {
        let (_dir, app) = app("asset");

        let response = request(app, "/health").await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn dash_route_generates_manifest() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/index.mpd").await;
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        assert!(status == StatusCode::OK && String::from_utf8_lossy(&body).contains("<MPD"));
    }

    #[tokio::test]
    async fn hls_master_route_generates_playlist() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/master.m3u8").await;
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        assert!(status == StatusCode::OK && body.starts_with(b"#EXTM3U"));
    }

    #[tokio::test]
    async fn hls_media_route_generates_track_playlist() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/video-main.m3u8").await;
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        assert!(
            status == StatusCode::OK && String::from_utf8_lossy(&body).contains("#EXT-X-ENDLIST")
        );
    }

    #[tokio::test]
    async fn hls_media_route_applies_sml_alias() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset,sml:10000)/video-main.m3u8").await;
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        assert!(
            status == StatusCode::OK
                && String::from_utf8_lossy(&body).contains("#EXT-X-TARGETDURATION:12")
        );
    }

    #[tokio::test]
    async fn initialization_route_returns_track_bytes() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/video-main/init.mp4").await;
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        assert!(status == StatusCode::OK && !body.is_empty());
    }

    #[tokio::test]
    async fn text_segment_route_returns_a_webvtt_document() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/text-nld/0.vtt").await;
        let status = response.status();
        let content_type = response
            .headers()
            .get("content-type")
            .map(|value| value.to_str().unwrap().to_string());
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        assert_eq!(
            (status, content_type.as_deref(), body.starts_with(b"WEBVTT")),
            (StatusCode::OK, Some("text/vtt"), true),
            "unexpected body: {}",
            String::from_utf8_lossy(&body)
        );
    }

    #[tokio::test]
    async fn text_segment_route_serves_the_cues_the_document_holds() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/text-nld/0.vtt").await;
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let document = String::from_utf8(body.to_vec()).unwrap();

        assert!(
            document.contains("00:00:00.000 --> 00:00:02.000\nHello"),
            "unexpected body: {document}"
        );
    }

    /// The same segment stays addressable as packaged bytes, which is what DASH
    /// asks for while HLS asks for the document.
    #[tokio::test]
    async fn media_segment_route_still_serves_a_text_track_as_packaged_bytes() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/text-nld/0.m4s").await;
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        assert_eq!((status, &body[4..8]), (StatusCode::OK, b"styp".as_slice()));
    }

    #[tokio::test]
    async fn hls_media_route_points_a_text_track_at_vtt_segments() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/text-nld.m3u8").await;
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let playlist = String::from_utf8(body.to_vec()).unwrap();

        assert!(
            playlist.contains("text-nld/0.vtt") && !playlist.contains("EXT-X-MAP"),
            "unexpected playlist: {playlist}"
        );
    }

    #[tokio::test]
    async fn hls_media_route_points_a_text_track_at_packaged_segments_on_request() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset,wvtt:!t)/text-nld.m3u8").await;
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let playlist = String::from_utf8(body.to_vec()).unwrap();

        assert!(
            playlist.contains("text-nld/0.m4s") && playlist.contains("text-nld/init.mp4"),
            "unexpected playlist: {playlist}"
        );
    }

    #[tokio::test]
    async fn catch_all_route_supports_nested_asset_path() {
        let (_dir, app) = app("foo/asset");

        let response = request(app, "/out/(asset:foo%2Fasset)/index.mpd").await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn malformed_options_return_bad_request() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/not-rison/index.mpd").await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unknown_track_returns_not_found() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset)/missing.m3u8").await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    async fn request(app: Router, uri: &str) -> Response {
        app.oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    fn app(asset: &str) -> (TempDir, Router) {
        let dir = tempfile::tempdir().unwrap();
        let asset_path = dir.path().join(format!("{asset}.json"));
        fs::create_dir_all(asset_path.parent().unwrap()).unwrap();
        fs::copy(
            format!("{FIXTURES}/video_avc_1080.mp4"),
            asset_path.parent().unwrap().join("video_avc_1080.mp4"),
        )
        .unwrap();
        fs::copy(
            format!("{FIXTURES}/audio_aac_nl_2.mp4"),
            asset_path.parent().unwrap().join("audio_aac_nl_2.mp4"),
        )
        .unwrap();
        fs::write(
            asset_path.parent().unwrap().join("subtitles_nld.vtt"),
            "WEBVTT\n\n00:00.000 --> 00:02.000\nHello\n",
        )
        .unwrap();
        fs::write(asset_path, asset_json()).unwrap();
        let op = Operator::new(Fs::default().root(dir.path().to_str().unwrap())).unwrap();
        (dir, build_router(op))
    }

    fn asset_json() -> &'static str {
        r#"{
          "tracks": [
            {
              "id": "video-main",
              "path": "video_avc_1080.mp4",
              "codec": "avc1.640028",
              "type": "video",
              "width": 1920,
              "height": 1080,
              "frame_rate": "25/1"
            },
            {
              "id": "audio-nld",
              "path": "audio_aac_nl_2.mp4",
              "codec": "mp4a.40.2",
              "type": "audio",
              "sample_rate": 48000,
              "channels": 2,
              "language": "nld"
            },
            {
              "id": "text-nld",
              "path": "subtitles_nld.vtt",
              "codec": "wvtt",
              "type": "text",
              "language": "nld"
            }
          ]
        }"#
    }
}
