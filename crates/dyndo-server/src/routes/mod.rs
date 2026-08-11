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
use dyndo_core::asset_descriptor::{AssetDescriptor, AssetDescriptorError};
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

async fn load_asset(op: &Operator, options: &Options) -> Result<AssetDescriptor, ServerError> {
    let mut asset = AssetDescriptor::read(op, &format!("{}.json", options.asset()))
        .await
        .map_err(|error| asset_error(options.asset(), error))?;
    options.apply_to(&mut asset);

    Ok(asset)
}

fn asset_error(asset: &str, error: AssetDescriptorError) -> ServerError {
    match &error {
        AssetDescriptorError::Storage(error) if error.kind() == opendal::ErrorKind::NotFound => {
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
    let asset = load_asset(&op, &options).await?;

    match resource {
        ("index", "mpd") => {
            let dash_options = options.dash_options();
            manifest::dash(&op, &asset, &dash_options, query.filter.as_ref()).await
        }
        ("master", "m3u8") => {
            let hls_options = options.hls_options();
            manifest::hls_master(&op, &asset, &hls_options, query.filter.as_ref()).await
        }
        (resource, "m3u8") => {
            let hls_options = options.hls_options();
            let Some((content_type, track_id)) = track_resource(resource) else {
                return Err(not_found());
            };
            match content_type {
                ContentType::Image => {
                    manifest::hls_images(&op, &asset, &hls_options, track_id).await
                }
                ContentType::Video | ContentType::Audio | ContentType::Text => {
                    manifest::hls_media(&op, &asset, &hls_options, track_id).await
                }
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
    let Some((content_type, track_id)) = track_resource(&track_id) else {
        return Err(not_found());
    };

    match (content_type, file_name, extension) {
        (ContentType::Video | ContentType::Audio | ContentType::Text, "init", "mp4") => {
            segment::initialization(&op, &asset, track_id).await
        }
        (ContentType::Video | ContentType::Audio | ContentType::Text, time, "m4s") => {
            segment::media(&op, &asset, track_id, segment_time(time, &file)?).await
        }
        (ContentType::Text, time, "vtt") => {
            segment::text(&op, &asset, track_id, segment_time(time, &file)?).await
        }
        (ContentType::Video, time, "jpg") => {
            segment::thumbnail(&op, &asset, &options, track_id, segment_time(time, &file)?).await
        }
        _ => Err(not_found()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContentType {
    Video,
    Audio,
    Text,
    Image,
}

fn track_resource(resource: &str) -> Option<(ContentType, &str)> {
    let (content_type, track_id) = resource.rsplit_once('_')?;
    let content_type = match content_type {
        "video" => ContentType::Video,
        "audio" => ContentType::Audio,
        "text" => ContentType::Text,
        "image" => ContentType::Image,
        _ => return None,
    };

    (!track_id.is_empty()).then_some((content_type, track_id))
}

fn segment_time(name: &str, file: &str) -> Result<u64, ServerError> {
    name.parse()
        .map_err(|_| ServerError::NotFound(file.to_string()))
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use axum::http::Uri;

    use super::{ContentType, ManifestQuery, track_resource};

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
    fn track_resource_reads_the_typed_identifier() {
        assert_eq!(
            track_resource("video_6b745be5-2791-5d95-8ce5-8f8bde29e2fe"),
            Some((ContentType::Video, "6b745be5-2791-5d95-8ce5-8f8bde29e2fe"))
        );
    }

    #[test]
    fn track_resource_rejects_unknown_content_types() {
        assert_eq!(track_resource("unknown_main"), None);
    }
}
