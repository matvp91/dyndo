//! Media resource handlers shared by every protocol: the init segment and the
//! media (sub)segments are the same CMAF bytes regardless of manifest format.

use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::asset::Track;
use dyndo_core::model::AssetModel;
use opendal::Operator;

use crate::error::ServerError;

/// `{repr}/init.mp4` — the representation's initialization segment.
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
