use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
};

use dyndo_core::generate_mpd;

use crate::config::Config;
use crate::error::ServerError;

use super::{load_asset, resolve_within};

pub(crate) async fn manifest(
    State(config): State<Arc<Config>>,
    Path(asset_id): Path<String>,
) -> Result<Response, ServerError> {
    let asset_dir = resolve_within(&config.assets_base_path, &asset_id)?;
    let asset = load_asset(&asset_dir, &asset_id).await?;
    // generate_mpd resolves each track's source against the asset dir at read time.
    let xml = generate_mpd(&asset, &asset_dir, true).await?;
    Ok(([(header::CONTENT_TYPE, "application/dash+xml")], xml).into_response())
}
