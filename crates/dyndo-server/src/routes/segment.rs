use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::track::Track;
use opendal::Operator;

use super::{RequestOptions, SegmentRouteOptions};
use crate::error::ServerError;

pub(super) async fn initialization(
    op: &Operator,
    request_options: &RequestOptions<SegmentRouteOptions>,
    track_id: &str,
) -> Result<Response, ServerError> {
    let (track, content_type) = read_track(op, request_options, track_id).await?;
    let bytes = track.read_initialization(op).await?;
    Ok(([(CONTENT_TYPE, content_type)], bytes).into_response())
}

pub(super) async fn media(
    op: &Operator,
    request_options: &RequestOptions<SegmentRouteOptions>,
    track_id: &str,
    file: &str,
) -> Result<Response, ServerError> {
    let time = file
        .strip_suffix(".m4s")
        .ok_or_else(|| ServerError::NotFound(file.to_string()))?
        .parse::<u64>()
        .map_err(|_| ServerError::NotFound(file.to_string()))?;
    let asset = request_options.read_asset(op).await?;
    let descriptor = asset
        .track(track_id)
        .ok_or_else(|| ServerError::NotFound(format!("track {track_id}")))?;
    let path = asset.track_path(descriptor);
    let track = Track::probe(op, &path, Some(descriptor.kind.clone())).await?;
    let mut start_time = track.earliest_presentation_time();

    for segment in track.segments(
        &asset.segment_boundaries,
        request_options.segment_options.min_segment_length_ms,
    ) {
        if start_time == time {
            let bytes = track.read_range(op, segment.byte_range()).await?;
            return Ok(([(CONTENT_TYPE, track.mime_type())], bytes).into_response());
        }
        start_time = start_time
            .checked_add(segment.duration())
            .ok_or_else(|| ServerError::SegmentTimeOverflow(track_id.to_string()))?;
    }

    Err(ServerError::NotFound(format!(
        "segment {file} for track {track_id}"
    )))
}

async fn read_track(
    op: &Operator,
    request_options: &RequestOptions<SegmentRouteOptions>,
    track_id: &str,
) -> Result<(Track, &'static str), ServerError> {
    let asset = request_options.read_asset(op).await?;
    let descriptor = asset
        .track(track_id)
        .ok_or_else(|| ServerError::NotFound(format!("track {track_id}")))?;
    let path = asset.track_path(descriptor);
    let track = Track::probe(op, &path, Some(descriptor.kind.clone())).await?;
    let content_type = track.mime_type();
    Ok((track, content_type))
}
