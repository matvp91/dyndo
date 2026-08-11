use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::asset::Asset;
use dyndo_core::track::cmaf::ResolvedCmafTrack;
use dyndo_core::track::{CmafRepresentationError, ResolvedTrack};
use opendal::Operator;

use super::resolve_track;
use crate::error::ServerError;

pub(super) async fn initialization(
    op: &Operator,
    asset: &Asset,
    track_id: &str,
) -> Result<Response, ServerError> {
    let track = resolve_track(op, asset, track_id, None).await?;
    let cmaf = cmaf_representation(&track, asset).await?;
    let bytes = cmaf
        .read_range(op, cmaf.init_segment().byte_range())
        .await?;

    Ok(([(CONTENT_TYPE, cmaf.kind().mime_type())], bytes).into_response())
}

pub(super) async fn media(
    op: &Operator,
    asset: &Asset,
    track_id: &str,
    time: u64,
) -> Result<Response, ServerError> {
    let track = resolve_track(op, asset, track_id, None).await?;
    let cmaf = cmaf_representation(&track, asset).await?;
    let segment = cmaf
        .served_segment(time, &asset.segment_options)
        .ok_or_else(|| ServerError::NotFound(format!("segment {time} for track {track_id}")))?;
    let bytes = cmaf.read_range(op, segment.byte_range()).await?;

    Ok(([(CONTENT_TYPE, cmaf.kind().mime_type())], bytes).into_response())
}

pub(super) async fn text(
    op: &Operator,
    asset: &Asset,
    track_id: &str,
    time: u64,
) -> Result<Response, ServerError> {
    let track = resolve_track(op, asset, track_id, None).await?;
    let timed_text = track
        .timed_text()
        .ok_or_else(|| ServerError::NotFound(format!("track {track_id}")))?;
    let text = timed_text
        .served_web_vtt_segment(time, &asset.segment_options)
        .await
        .map_err(CmafRepresentationError::from)?
        .ok_or_else(|| ServerError::NotFound(format!("segment {time} for track {track_id}")))?;

    Ok(([(CONTENT_TYPE, "text/vtt")], text).into_response())
}

async fn cmaf_representation(
    track: &ResolvedTrack,
    asset: &Asset,
) -> Result<ResolvedCmafTrack, ServerError> {
    track
        .cmaf_representation(&asset.segment_options)
        .await
        .map_err(Into::into)
}

/// Serves the thumbnail sprite named by the DASH `$Time$` substitution.
pub(super) async fn thumbnail(
    op: &Operator,
    asset: &Asset,
    thumbnail_id: &str,
    time: u64,
) -> Result<Response, ServerError> {
    let track = resolve_track(op, asset, thumbnail_id, None).await?;
    let thumbnail = track
        .thumbnail()
        .ok_or_else(|| ServerError::NotFound(format!("thumbnail {thumbnail_id}")))?;
    let Some(bytes) = thumbnail.jpeg(op, time).await? else {
        return Err(ServerError::NotFound("thumbnail".to_string()));
    };

    Ok(([(CONTENT_TYPE, "image/jpeg")], bytes).into_response())
}
