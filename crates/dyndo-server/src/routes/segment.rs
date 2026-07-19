//! Media resource handlers shared by every protocol: the init segment and the
//! media (sub)segments are the same CMAF bytes regardless of manifest format.

use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::asset::Asset;
use opendal::Operator;

use super::find_track;
use crate::error::ServerError;

/// `{repr}/init.mp4` — the representation's initialization segment.
pub(super) async fn init_segment(
    op: &Operator,
    asset_path: &str,
    repr: &str,
) -> Result<Response, ServerError> {
    let asset = Asset::read(op, asset_path).await?;
    let track = find_track(&asset, repr)?;
    let bytes = track.read_init_segment(op, asset_path).await?;
    Ok(([(CONTENT_TYPE, track.mime_type())], bytes).into_response())
}

/// `{repr}/{time}.m4s` — the media segment starting at presentation `time`.
pub(super) async fn media_segment(
    op: &Operator,
    asset_path: &str,
    repr: &str,
    seg: &str,
) -> Result<Response, ServerError> {
    let time: u64 = seg
        .strip_suffix(".m4s")
        .ok_or_else(|| ServerError::NotFound(format!("unknown segment: {seg}")))?
        .parse()
        .map_err(|_| ServerError::BadRequest(format!("invalid segment time: {seg}")))?;
    let asset = Asset::read(op, asset_path).await?;
    let track = find_track(&asset, repr)?;
    let bytes = track
        .read_segment(
            op,
            asset_path,
            time,
            &asset.segment_boundaries_ms,
            asset.min_segment_length_ms,
        )
        .await?
        .ok_or_else(|| ServerError::NotFound(format!("no segment at time {time}")))?;
    Ok(([(CONTENT_TYPE, track.mime_type())], bytes).into_response())
}
