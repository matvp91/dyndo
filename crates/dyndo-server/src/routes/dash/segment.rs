use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::{load_asset, read_header, read_init_segment, read_segment, CmafHeader, LocalFile};

use super::resolve_within;
use crate::config::Config;
use crate::error::ServerError;

/// `GET /{asset}/dash/{repr}/init.mp4` — the representation's init segment.
pub(crate) async fn init_segment(
    State(config): State<Arc<Config>>,
    Path((asset_id, repr)): Path<(String, String)>,
) -> Result<Response, ServerError> {
    let (source, header) = locate(&config, &asset_id, &repr).await?;
    let bytes = read_init_segment(&source, &header).await?;
    Ok(([(CONTENT_TYPE, header.stream.mime_type())], bytes).into_response())
}

/// `GET /{asset}/dash/{repr}/{seg}` — the media segment named `{time}.m4s`.
pub(crate) async fn media_segment(
    State(config): State<Arc<Config>>,
    Path((asset_id, repr, seg)): Path<(String, String, String)>,
) -> Result<Response, ServerError> {
    // Resolve the representation first so an unknown asset/repr is a 404 even when
    // the segment name is also malformed.
    let (source, header) = locate(&config, &asset_id, &repr).await?;
    let time: u64 = seg
        .strip_suffix(".m4s")
        .ok_or_else(|| ServerError::NotFound(format!("unknown segment: {seg}")))?
        .parse()
        .map_err(|_| ServerError::BadRequest(format!("invalid segment time: {seg}")))?;
    let bytes = read_segment(&source, &header, time)
        .await?
        .ok_or_else(|| ServerError::NotFound(format!("no segment at time {time}")))?;
    Ok(([(CONTENT_TYPE, header.stream.mime_type())], bytes).into_response())
}

/// Resolve `{asset}/{repr}` to the representation's source file and its parsed
/// CMAF header, which carries both the segment map and the media (MIME) type.
async fn locate(
    config: &Config,
    asset_id: &str,
    repr: &str,
) -> Result<(LocalFile, CmafHeader), ServerError> {
    let asset_dir = resolve_within(&config.assets_base_path, asset_id)?;
    let asset = load_asset(&asset_dir, asset_id).await?;
    let track = asset
        .track(repr)
        .ok_or_else(|| ServerError::NotFound(format!("no representation {repr}")))?;
    let abs = resolve_within(&asset_dir, track.source())?;
    let key = abs.to_string_lossy().into_owned();
    let source = LocalFile::new(&abs);
    let header = read_header(&source, &key).await?;
    Ok((source, header))
}
