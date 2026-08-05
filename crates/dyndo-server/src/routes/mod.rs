mod segment;
mod transport;

use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
    routing::get,
};
use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::segment::SegmentOptions;
use dyndo_dash::options::DashOptions;
use dyndo_hls::options::HlsOptions;
use opendal::Operator;
use serde::{Deserialize, de::DeserializeOwned};
use tower_http::cors::{Any, CorsLayer};

use crate::error::ServerError;

#[derive(Debug, Deserialize)]
struct RequestTransportOptions<T> {
    #[serde(alias = "a")]
    asset: String,
    #[serde(flatten)]
    segment_options: SegmentOptions,
    #[serde(flatten)]
    transport_options: T,
}

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
            let request_options = parse_request_options::<DashOptions>(&options)?;
            transport::dash_manifest(&op, &request_options).await
        }
        // Generate the HLS multivariant playlist.
        "master.m3u8" => {
            let request_options = parse_request_options::<HlsOptions>(&options)?;
            transport::hls_master(&op, &request_options).await
        }
        // Generate the media playlist for the track named by the filename.
        resource if !resource.contains('/') && resource.ends_with(".m3u8") => {
            let track_id = resource
                .strip_suffix(".m3u8")
                .ok_or_else(|| ServerError::NotFound(resource.to_string()))?;
            let request_options = parse_request_options::<HlsOptions>(&options)?;
            transport::hls_media(&op, &request_options, track_id).await
        }
        // Serve initialization or media bytes for the track named by the path.
        resource => {
            let request_options = parse_request_options::<()>(&options)?;
            let (track_id, file) = resource
                .split_once('/')
                .ok_or_else(|| ServerError::NotFound(resource.to_string()))?;
            if file == "init.mp4" {
                segment::initialization(&op, &request_options, track_id).await
            } else {
                segment::media(&op, &request_options, track_id, file).await
            }
        }
    }
}

fn parse_request_options<T: DeserializeOwned>(
    options: &str,
) -> Result<RequestTransportOptions<T>, ServerError> {
    rison::from_str(options).map_err(|error| ServerError::InvalidOptions(error.to_string()))
}

async fn read_asset(op: &Operator, asset: &str) -> Result<AssetDescriptor, ServerError> {
    Ok(AssetDescriptor::read(op, &format!("{asset}.json")).await?)
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

    #[test]
    fn parse_request_options_accepts_a_nested_asset_path() {
        let request_options = parse_request_options::<DashOptions>("(asset:foo/asset)").unwrap();

        assert_eq!(request_options.asset, "foo/asset");
    }

    #[test]
    fn parse_request_options_accepts_asset_alias() {
        let request_options = parse_request_options::<DashOptions>("(a:foo/asset)").unwrap();

        assert_eq!(request_options.asset, "foo/asset");
    }

    #[test]
    fn parse_request_options_accepts_min_segment_length() {
        let request_options =
            parse_request_options::<HlsOptions>("(asset:asset,min_segment_length:3000)").unwrap();

        assert_eq!(request_options.segment_options.min_segment_length_ms, 3000);
    }

    #[test]
    fn parse_request_options_accepts_msl_alias() {
        let request_options =
            parse_request_options::<HlsOptions>("(asset:asset,msl:3000)").unwrap();

        assert_eq!(request_options.segment_options.min_segment_length_ms, 3000);
    }

    #[test]
    fn parse_request_options_accepts_compact_alias() {
        let request_options = parse_request_options::<DashOptions>("(asset:asset,c:!t)").unwrap();

        assert!(request_options.transport_options.compact);
    }

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
    async fn hls_media_route_applies_msl_alias() {
        let (_dir, app) = app("asset");

        let response = request(app, "/out/(asset:asset,msl:10000)/video-main.m3u8").await;
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
            }
          ]
        }"#
    }
}
