mod context;
mod manifest;
mod manifest_query;
mod segment;

use axum::{
    Router,
    extract::{Path, Query, State},
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
use crate::routes::manifest_query::ManifestQuery;

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
    Path((options, resource)): Path<(String, String)>,
    Query(query): Query<ManifestQuery>,
) -> Result<Response, ServerError> {
    let not_found = || ServerError::NotFound(resource.clone());

    match resource.rsplit_once('.').ok_or_else(not_found)? {
        ("index", "mpd") => {
            let context = parse_context::<DashOptions>(&options)?;
            let filter = query.resolve()?;
            manifest::dash(&op, &context, filter.as_ref()).await
        }
        ("master", "m3u8") => {
            let context = parse_context::<HlsOptions>(&options)?;
            let filter = query.resolve()?;
            manifest::hls_master(&op, &context, filter.as_ref()).await
        }
        (track_id, "m3u8") => {
            let context = parse_context::<HlsOptions>(&options)?;
            manifest::hls_media(&op, &context, track_id).await
        }
        _ => Err(not_found()),
    }
}

async fn track_file(
    State(op): State<Operator>,
    Path((options, track_id, file)): Path<(String, String, String)>,
) -> Result<Response, ServerError> {
    let context = parse_context::<()>(&options)?;
    let not_found = || ServerError::NotFound(file.clone());

    match file.rsplit_once('.').ok_or_else(not_found)? {
        ("init", "mp4") => segment::initialization(&op, &context, &track_id).await,
        (time, "m4s") => segment::media(&op, &context, &track_id, segment_time(time, &file)?).await,
        (time, "vtt") => segment::text(&op, &context, &track_id, segment_time(time, &file)?).await,
        _ => Err(not_found()),
    }
}

fn segment_time(name: &str, file: &str) -> Result<u64, ServerError> {
    name.parse()
        .map_err(|_| ServerError::NotFound(file.to_string()))
}
