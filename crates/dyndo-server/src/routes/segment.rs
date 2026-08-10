use std::ops::Range;

use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::reader::Reader;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::served_segment::ServedSegment;
use dyndo_core::text::Subtitle;
use dyndo_core::track::Track;
use opendal::Operator;

use super::context::RequestContext;
use crate::error::ServerError;

pub(super) async fn initialization(
    op: &Operator,
    context: &RequestContext<()>,
    track_id: &str,
) -> Result<Response, ServerError> {
    let (track, segment_options) = read_track(op, context, track_id).await?;
    let bytes = Reader::new(op, &track, &segment_options)
        .read_initialization()
        .await?;

    Ok(([(CONTENT_TYPE, track.kind().mime_type())], bytes).into_response())
}

pub(super) async fn media(
    op: &Operator,
    context: &RequestContext<()>,
    track_id: &str,
    time: u64,
) -> Result<Response, ServerError> {
    let (track, segment_options, range) = locate(op, context, track_id, time).await?;
    let bytes = Reader::new(op, &track, &segment_options)
        .read_range(range)
        .await?;

    Ok(([(CONTENT_TYPE, track.kind().mime_type())], bytes).into_response())
}

pub(super) async fn text(
    op: &Operator,
    context: &RequestContext<()>,
    track_id: &str,
    time: u64,
) -> Result<Response, ServerError> {
    let (track, segment_options, range) = locate(op, context, track_id, time).await?;
    let reader = Reader::new(op, &track, &segment_options);
    let initialization = reader.read_initialization().await?;
    let segment = reader.read_range(range).await?;
    let mut bytes = Vec::with_capacity(initialization.len() + segment.len());
    bytes.extend_from_slice(&initialization);
    bytes.extend_from_slice(&segment);
    let subtitle = Subtitle::from_wvtt(&bytes)?;

    Ok(([(CONTENT_TYPE, "text/vtt")], subtitle.to_vtt_text()).into_response())
}

async fn locate(
    op: &Operator,
    context: &RequestContext<()>,
    track_id: &str,
    time: u64,
) -> Result<(Track, SegmentOptions, Range<u64>), ServerError> {
    let (track, segment_options) = read_track(op, context, track_id).await?;
    let range = ServedSegment::group(
        track.segments(),
        segment_options.min_length,
        &segment_options.boundaries,
    )
    .into_iter()
    .find(|segment| segment.unscaled_start_time() == time)
    .map(|segment| segment.byte_range())
    .ok_or_else(|| ServerError::NotFound(format!("segment {time} for track {track_id}")))?;

    Ok((track, segment_options, range))
}

async fn read_track(
    op: &Operator,
    context: &RequestContext<()>,
    track_id: &str,
) -> Result<(Track, SegmentOptions), ServerError> {
    let asset = context.read_asset(op).await?;
    let descriptor = asset
        .find_track_by_id(track_id)
        .ok_or_else(|| ServerError::NotFound(format!("track {track_id}")))?;
    let path = asset.track_path(descriptor);
    let segment_options = asset.segment_options.clone();
    let track = Track::probe(op, &path, Some(descriptor), &segment_options).await?;

    Ok((track, segment_options))
}
