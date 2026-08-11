use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::asset::Asset;
use opendal::Operator;

use super::track_resolver::{LocatedSegment, RequestTrack, TrackResolver};
use crate::error::ServerError;

pub(super) async fn initialization(
    op: &Operator,
    asset: &Asset,
    track_id: &str,
) -> Result<Response, ServerError> {
    let track = TrackResolver::new(op, asset).resolve(track_id).await?;
    let bytes = read_range(op, &track, track.cmaf().init_segment().byte_range()).await?;

    Ok(([(CONTENT_TYPE, track.cmaf().kind().mime_type())], bytes).into_response())
}

pub(super) async fn media(
    op: &Operator,
    asset: &Asset,
    track_id: &str,
    time: u64,
) -> Result<Response, ServerError> {
    let LocatedSegment {
        track, byte_range, ..
    } = TrackResolver::new(op, asset)
        .locate_segment(track_id, time)
        .await?;
    let bytes = read_range(op, &track, byte_range).await?;

    Ok(([(CONTENT_TYPE, track.cmaf().kind().mime_type())], bytes).into_response())
}

pub(super) async fn text(
    op: &Operator,
    asset: &Asset,
    track_id: &str,
    time: u64,
) -> Result<Response, ServerError> {
    let LocatedSegment {
        track,
        start_time,
        end_time,
        ..
    } = TrackResolver::new(op, asset)
        .locate_segment(track_id, time)
        .await?;
    let text = track
        .web_vtt_segment(start_time, end_time)
        .ok_or_else(|| ServerError::NotFound(format!("segment {time} for track {track_id}")))?;

    Ok(([(CONTENT_TYPE, "text/vtt")], text).into_response())
}

async fn read_range(
    op: &Operator,
    track: &RequestTrack,
    range: std::ops::Range<u64>,
) -> Result<bytes::Bytes, ServerError> {
    Ok(track.cmaf().read_range(op, range).await?)
}

/// Serves the thumbnail sprite named by the DASH `$Time$` substitution.
pub(super) async fn thumbnail(
    op: &Operator,
    asset: &Asset,
    thumbnail_id: &str,
    time: u64,
) -> Result<Response, ServerError> {
    let configured = asset
        .find_thumbnail_track_by_id(thumbnail_id)
        .ok_or_else(|| ServerError::NotFound(format!("thumbnail {thumbnail_id}")))?;
    let source_tracks = TrackResolver::new(op, asset).resolve_sources().await?;
    let thumbnail = asset
        .resolve_thumbnails(&source_tracks)
        .into_iter()
        .find(|track| track.id() == configured.id);
    let Some(thumbnail) = thumbnail else {
        return Err(ServerError::NotFound("thumbnail".to_string()));
    };
    let Some(bytes) = thumbnail.jpeg(op, time).await? else {
        return Err(ServerError::NotFound("thumbnail".to_string()));
    };

    Ok(([(CONTENT_TYPE, "image/jpeg")], bytes).into_response())
}
