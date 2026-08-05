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
use opendal::Operator;
use serde::{Deserialize, Deserializer};
use tower_http::cors::{Any, CorsLayer};

use crate::error::ServerError;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputParameters {
    asset: String,
    min_segment_length: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_segment_boundaries")]
    segment_boundaries: Option<Vec<i32>>,
}

fn deserialize_segment_boundaries<'de, D>(deserializer: D) -> Result<Option<Vec<i32>>, D::Error>
where
    D: Deserializer<'de>,
{
    Vec::deserialize(deserializer).map(Some)
}

struct OutputRoute<'a> {
    parameters: OutputParameters,
    resource: &'a str,
}

impl OutputParameters {
    fn asset_path(&self) -> Result<String, ServerError> {
        let valid = !self.asset.is_empty()
            && !self.asset.starts_with('/')
            && !self.asset.ends_with('/')
            && !self.asset.ends_with(".json")
            && !self.asset.contains('\\')
            && self
                .asset
                .split('/')
                .all(|component| !matches!(component, "" | "." | ".."));
        if !valid {
            return Err(ServerError::InvalidAssetPath(self.asset.clone()));
        }
        Ok(format!("{}.json", self.asset))
    }

    async fn read_asset(&self, op: &Operator) -> Result<AssetDescriptor, ServerError> {
        let mut asset = AssetDescriptor::read(op, &self.asset_path()?).await?;
        if let Some(min_segment_length) = self.min_segment_length {
            asset.min_segment_length = u64::try_from(min_segment_length).map_err(|_| {
                ServerError::InvalidOptions("min_segment_length cannot be negative".into())
            })?;
        }
        if let Some(segment_boundaries) = &self.segment_boundaries {
            asset.segment_boundaries = segment_boundaries
                .iter()
                .map(|&boundary| {
                    u64::try_from(boundary).map_err(|_| {
                        ServerError::InvalidOptions(
                            "segment_boundaries cannot contain negative values".into(),
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
        }
        Ok(asset)
    }
}

pub(crate) fn build_router(op: Operator) -> Router {
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any);
    Router::new()
        .route("/health", get(health))
        .route("/out/{*path}", get(dispatch))
        .with_state(op)
        .layer(cors)
}

async fn health() -> StatusCode {
    StatusCode::OK
}

async fn dispatch(
    State(op): State<Operator>,
    Path(path): Path<String>,
) -> Result<Response, ServerError> {
    let route = parse_route(path.strip_prefix('/').unwrap_or(&path))?;

    match route.resource {
        "index.mpd" => transport::dash_manifest(&op, &route.parameters).await,
        "master.m3u8" => transport::hls_master(&op, &route.parameters).await,
        resource if !resource.contains('/') && resource.ends_with(".m3u8") => {
            let track_id = resource
                .strip_suffix(".m3u8")
                .ok_or_else(|| ServerError::NotFound(resource.to_string()))?;
            transport::hls_media(&op, &route.parameters, track_id).await
        }
        resource => {
            let (track_id, file) = resource
                .split_once('/')
                .ok_or_else(|| ServerError::NotFound(resource.to_string()))?;
            if file == "init.mp4" {
                segment::initialization(&op, &route.parameters, track_id).await
            } else {
                segment::media(&op, &route.parameters, track_id, file).await
            }
        }
    }
}

fn parse_route(path: &str) -> Result<OutputRoute<'_>, ServerError> {
    if !path.starts_with('(') {
        return Err(ServerError::InvalidOptions(
            "route must start with a Rison object".into(),
        ));
    }
    let options_end = closing_parenthesis(path).ok_or_else(|| {
        ServerError::InvalidOptions("Rison object has no matching closing parenthesis".into())
    })?;
    let options = &path[..=options_end];
    let resource = path
        .get(options_end + 1..)
        .and_then(|suffix| suffix.strip_prefix('/'))
        .filter(|resource| !resource.is_empty())
        .ok_or_else(|| ServerError::NotFound("missing output resource".into()))?;
    let parameters =
        rison::from_str(options).map_err(|error| ServerError::InvalidOptions(error.to_string()))?;

    Ok(OutputRoute {
        parameters,
        resource,
    })
}

fn closing_parenthesis(value: &str) -> Option<usize> {
    let mut depth = 0_u32;
    let mut quoted = false;
    let mut escaped = false;

    for (index, character) in value.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if character == '!' {
                escaped = true;
            } else if character == '\'' {
                quoted = false;
            }
            continue;
        }
        match character {
            '\'' => quoted = true,
            '(' => depth = depth.checked_add(1)?,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
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
    fn parse_route_accepts_a_nested_asset_path() {
        let route = parse_route("(asset:foo/asset)/index.mpd").unwrap();

        assert_eq!(route.parameters.asset, "foo/asset");
    }

    #[test]
    fn parse_route_accepts_segment_options() {
        let route = parse_route(
            "(asset:asset,min_segment_length:3000,segment_boundaries:!(1000,2000))/master.m3u8",
        )
        .unwrap();

        assert_eq!(route.parameters.segment_boundaries, Some(vec![1000, 2000]));
    }

    #[test]
    fn asset_path_rejects_parent_traversal() {
        let parameters = OutputParameters {
            asset: "foo/../asset".into(),
            min_segment_length: None,
            segment_boundaries: None,
        };

        assert!(matches!(
            parameters.asset_path(),
            Err(ServerError::InvalidAssetPath(_))
        ));
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

        let response = request(app, "/out/(asset:foo/asset)/index.mpd").await;

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
