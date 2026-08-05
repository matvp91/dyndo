use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::segment::SegmentOptions;
use dyndo_core::track::Track;
use opendal::Operator;

use super::{RequestTransportOptions, read_asset};
use crate::error::ServerError;

pub(super) async fn initialization(
    op: &Operator,
    request_options: &RequestTransportOptions<()>,
    track_id: &str,
) -> Result<Response, ServerError> {
    let (segment_options, track, content_type) = read_track(op, request_options, track_id).await?;
    let bytes = track.read_initialization(op, &segment_options).await?;
    Ok(([(CONTENT_TYPE, content_type)], bytes).into_response())
}

pub(super) async fn media(
    op: &Operator,
    request_options: &RequestTransportOptions<()>,
    track_id: &str,
    file: &str,
) -> Result<Response, ServerError> {
    let time = file
        .strip_suffix(".m4s")
        .ok_or_else(|| ServerError::NotFound(file.to_string()))?
        .parse::<u64>()
        .map_err(|_| ServerError::NotFound(file.to_string()))?;
    let asset = read_asset(op, &request_options.asset).await?;
    let descriptor = asset
        .track(track_id)
        .ok_or_else(|| ServerError::NotFound(format!("track {track_id}")))?;
    let path = asset.track_path(descriptor);
    let mut segment_options = request_options.segment_options.clone();
    segment_options.segment_boundaries = asset.segment_boundaries.clone();
    let track = Track::probe(op, &path, Some(descriptor.kind.clone()), &segment_options).await?;
    let mut start_time = track.earliest_presentation_time();

    for segment in track.segments(&segment_options) {
        if start_time == time {
            let bytes = track
                .read_range(op, &segment_options, segment.byte_range())
                .await?;
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
    request_options: &RequestTransportOptions<()>,
    track_id: &str,
) -> Result<(SegmentOptions, Track, &'static str), ServerError> {
    let asset = read_asset(op, &request_options.asset).await?;
    let descriptor = asset
        .track(track_id)
        .ok_or_else(|| ServerError::NotFound(format!("track {track_id}")))?;
    let path = asset.track_path(descriptor);
    let mut segment_options = request_options.segment_options.clone();
    segment_options.segment_boundaries = asset.segment_boundaries.clone();
    let track = Track::probe(op, &path, Some(descriptor.kind.clone()), &segment_options).await?;
    let content_type = track.mime_type();
    Ok((segment_options, track, content_type))
}
