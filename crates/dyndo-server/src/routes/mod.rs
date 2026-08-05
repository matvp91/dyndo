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
    use super::*;

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
}
