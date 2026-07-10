//! DASH manifest generation — the protocol-specific piece of the pipeline. Its
//! segments are served by [`super::serve`], which is shared across protocols; a
//! future `hls` module would sit alongside this one, differing only here.

use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::generate_mpd;

use super::serve::load;
use crate::config::Config;
use crate::error::ServerError;

/// `index.mpd` — the asset's DASH manifest.
pub(super) async fn manifest(config: &Config, asset_path: &str) -> Result<Response, ServerError> {
    let (asset, asset_dir) = load(config, asset_path).await?;
    // generate_mpd resolves each track's source against the asset dir at read time.
    let xml = generate_mpd(&asset, &asset_dir, true).await?;
    Ok(([(CONTENT_TYPE, "application/dash+xml")], xml).into_response())
}
