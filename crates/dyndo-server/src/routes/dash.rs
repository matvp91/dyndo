//! Every streaming route handler. The DASH manifest is protocol-specific; the
//! init and media segments are the same CMAF bytes for any protocol and, now
//! that the core makes serving them a couple of lines, they live here too. A
//! future `hls` module would sit alongside, differing only in the manifest.

use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::asset::{Asset, Track};
use dyndo_core::model::AssetModel;
use opendal::Operator;

use crate::error::ServerError;

/// `index.mpd` — the asset's DASH manifest. It describes every representation,
/// so the whole asset is built (each track's header parsed).
pub(super) async fn manifest(op: &Operator, asset_path: &str) -> Result<Response, ServerError> {
    let model = AssetModel::read(op, asset_path).await?;
    let asset = Asset::from_model(op, model, asset_path).await?;
    let xml = dyndo_dash::generate_mpd(&asset, true);
    Ok(([(CONTENT_TYPE, "application/dash+xml")], xml).into_response())
}

/// `{repr}/init.mp4` — the representation's init segment.
pub(super) async fn init_segment(
    op: &Operator,
    asset_path: &str,
    repr: &str,
) -> Result<Response, ServerError> {
    let model = AssetModel::read(op, asset_path).await?;
    let source = model
        .tracks
        .iter()
        .find(|t| t.id() == repr)
        .ok_or_else(|| ServerError::NotFound(format!("no representation {repr}")))?;
    let track = Track::from_path(op, source.path(), asset_path).await?;
    let bytes = track.init_segment_bytes(op).await?;
    Ok(([(CONTENT_TYPE, track.metadata.mime_type())], bytes).into_response())
}

/// `{repr}/{time}.m4s` — the media segment starting at presentation `time`.
pub(super) async fn media_segment(
    op: &Operator,
    asset_path: &str,
    repr: &str,
    seg: &str,
) -> Result<Response, ServerError> {
    // Resolve the representation first, so an unknown asset/repr is a 404 even
    // when the segment name is also malformed.
    let model = AssetModel::read(op, asset_path).await?;
    let source = model
        .tracks
        .iter()
        .find(|t| t.id() == repr)
        .ok_or_else(|| ServerError::NotFound(format!("no representation {repr}")))?;
    let track = Track::from_path(op, source.path(), asset_path).await?;
    let time: u64 = seg
        .strip_suffix(".m4s")
        .ok_or_else(|| ServerError::NotFound(format!("unknown segment: {seg}")))?
        .parse()
        .map_err(|_| ServerError::BadRequest(format!("invalid segment time: {seg}")))?;
    let bytes = track
        .segment_bytes(op, time)
        .await?
        .ok_or_else(|| ServerError::NotFound(format!("no segment at time {time}")))?;
    Ok(([(CONTENT_TYPE, track.metadata.mime_type())], bytes).into_response())
}
