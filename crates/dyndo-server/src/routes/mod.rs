pub(crate) mod filter;
mod manifest;
mod options;
mod segment;

use axum::{Router, http::StatusCode, routing::get};
use dyndo_core::asset::{Asset, AssetError, ResolvedAsset};
use dyndo_core::track::ResolvedTrack;
use opendal::Operator;
use tower_http::cors::{Any, CorsLayer};

use self::filter::Filter;
use self::manifest::ManifestRoute;
use self::options::Options;
use self::segment::SegmentRoute;
use crate::error::ServerError;

async fn load_asset(op: &Operator, options: &Options) -> Result<Asset, ServerError> {
    Asset::read(op, &format!("{}.json", options.asset()))
        .await
        .map_err(|error| asset_error(options.asset(), error))
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
        .manifest_route()
        .segment_route()
        .with_state(op)
        .layer(cors)
}

async fn health() -> StatusCode {
    StatusCode::OK
}

pub(in crate::routes) async fn resolve_asset(
    op: &Operator,
    asset: &Asset,
    filter: Option<&Filter>,
) -> Result<ResolvedAsset, ServerError> {
    let mut resolved = asset.resolve(op).await?;
    if let Some(filter) = filter {
        filter
            .apply(&mut resolved)
            .map_err(|error| ServerError::NotFound(error.to_string()))?;
    }
    Ok(resolved)
}

pub(in crate::routes) async fn resolve_track(
    op: &Operator,
    asset: &Asset,
    id: &str,
    filter: Option<&Filter>,
) -> Result<ResolvedTrack, ServerError> {
    let track = asset
        .resolve_track(op, id)
        .await?
        .ok_or_else(|| ServerError::NotFound(format!("track {id}")))?;
    if filter.is_some_and(|filter| !filter.matches(&track)) {
        return Err(ServerError::NotFound(format!("track {id}")));
    }
    Ok(track)
}
