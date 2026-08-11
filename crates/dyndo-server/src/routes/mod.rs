pub(crate) mod filter;
mod manifest;
mod options;
mod segment;
mod track_resolver;

use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
    routing::get,
};
use dyndo_core::asset::{Asset, AssetError};
use opendal::Operator;
use serde::Deserialize;
use tower_http::cors::{Any, CorsLayer};

use self::filter::Filter;
use self::options::Options;
use crate::error::ServerError;

// Rejecting unknown fields prevents an unencoded `&&` from silently truncating a filter.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestQuery {
    filter: Option<Filter>,
}

async fn load_asset(op: &Operator, options: &Options) -> Result<Asset, ServerError> {
    let mut asset = Asset::read(op, &format!("{}.json", options.asset()))
        .await
        .map_err(|error| asset_error(options.asset(), error))?;
    options.apply_to(&mut asset);

    Ok(asset)
}

fn asset_error(asset: &str, error: AssetError) -> ServerError {
    match &error {
        AssetError::Storage(error) if error.kind() == opendal::ErrorKind::NotFound => {
            ServerError::NotFound(format!("asset {asset}"))
        }
        _ => error.into(),
    }
}

pub(crate) fn build_router(op: Operator) -> Router {
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any);
    Router::new()
        .route("/health", get(health))
        .route("/out/{options}/{resource}", get(manifest))
        .route("/out/{options}/{track_id}/{file}", get(track_file))
        .with_state(op)
        .layer(cors)
}

async fn health() -> StatusCode {
    StatusCode::OK
}

async fn manifest(
    State(op): State<Operator>,
    Path((encoded_options, resource)): Path<(String, String)>,
    Query(query): Query<ManifestQuery>,
) -> Result<Response, ServerError> {
    let not_found = || ServerError::NotFound(resource.clone());
    let resource = resource.rsplit_once('.').ok_or_else(not_found)?;
    let options = Options::parse(&encoded_options)?;
    let source_asset = load_asset(&op, &options).await?;
    let mut asset = source_asset.clone();
    if let Some(filter) = &query.filter {
        filter
            .apply(&mut asset)
            .map_err(|error| ServerError::NotFound(error.to_string()))?;
    }

    match resource {
        ("index", "mpd") => {
            let dash_options = options.dash_options();
            manifest::dash(&op, &source_asset, &asset, &dash_options).await
        }
        ("master", "m3u8") => {
            let hls_options = options.hls_options();
            manifest::hls_master(&op, &source_asset, &asset, &hls_options).await
        }
        (resource, "m3u8") => {
            let hls_options = options.hls_options();
            let Some(id) = resource_id(resource) else {
                return Err(not_found());
            };
            if resource.starts_with("image_") {
                manifest::hls_images(&op, &source_asset, &asset, id).await
            } else {
                manifest::hls_media(&op, &asset, &hls_options, id).await
            }
        }
        _ => Err(not_found()),
    }
}

async fn track_file(
    State(op): State<Operator>,
    Path((encoded_options, track_id, file)): Path<(String, String, String)>,
) -> Result<Response, ServerError> {
    let not_found = || ServerError::NotFound(file.clone());
    let (file_name, extension) = file.rsplit_once('.').ok_or_else(not_found)?;
    let options = Options::parse(&encoded_options)?;
    let asset = load_asset(&op, &options).await?;
    let track_id = resource_id(&track_id).ok_or_else(not_found)?;

    match (file_name, extension) {
        ("init", "mp4") => segment::initialization(&op, &asset, track_id).await,
        (time, "m4s") => segment::media(&op, &asset, track_id, segment_time(time, &file)?).await,
        (time, "vtt") => segment::text(&op, &asset, track_id, segment_time(time, &file)?).await,
        (time, "jpg") => {
            segment::thumbnail(&op, &asset, track_id, segment_time(time, &file)?).await
        }
        _ => Err(not_found()),
    }
}

fn resource_id(resource: &str) -> Option<&str> {
    resource
        .rsplit_once('_')
        .map(|(_, id)| id)
        .filter(|id| !id.is_empty())
}

fn segment_time(name: &str, file: &str) -> Result<u64, ServerError> {
    name.parse()
        .map_err(|_| ServerError::NotFound(file.to_string()))
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use axum::http::Uri;

    use super::{ManifestQuery, resource_id};

    #[test]
    fn manifest_query_deserializes_filter() {
        let uri: Uri = "/?filter=type%3D%3Dvideo".parse().unwrap();
        let Query(query) = Query::<ManifestQuery>::try_from_uri(&uri).unwrap();

        assert!(query.filter.is_some());
    }

    #[test]
    fn manifest_query_rejects_invalid_filter() {
        let uri: Uri = "/?filter=type%3Evideo".parse().unwrap();

        assert!(Query::<ManifestQuery>::try_from_uri(&uri).is_err());
    }

    #[test]
    fn resource_id_reads_the_last_underscore_separated_chunk() {
        assert_eq!(resource_id("video_6b745be5"), Some("6b745be5"));
    }

    #[test]
    fn resource_id_rejects_resources_without_an_identifier() {
        assert_eq!(resource_id("video_"), None);
    }
}
