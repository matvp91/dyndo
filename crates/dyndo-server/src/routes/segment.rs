use std::ops::Range;

use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::segment::SegmentOptions;
use dyndo_core::track::Track;
use dyndo_text::{fragmenter, wvtt};
use opendal::Operator;

use super::context::RequestContext;
use crate::error::ServerError;

pub(super) async fn initialization(
    op: &Operator,
    context: &RequestContext<()>,
    track_id: &str,
) -> Result<Response, ServerError> {
    let (track, segment_options) = read_track(op, context, track_id).await?;
    let bytes = track.read_initialization(op, &segment_options).await?;

    Ok(([(CONTENT_TYPE, track.mime_type())], bytes).into_response())
}

/// Serves one segment as the packaged CMAF bytes it is stored as.
pub(super) async fn media(
    op: &Operator,
    context: &RequestContext<()>,
    track_id: &str,
    file: &str,
) -> Result<Response, ServerError> {
    let (track, segment_options, range) = locate(op, context, track_id, file, ".m4s").await?;
    let bytes = track.read_range(op, &segment_options, range).await?;

    Ok(([(CONTENT_TYPE, track.mime_type())], bytes).into_response())
}

/// Serves one segment of a text track as a WebVTT document, read back out of the
/// bytes [`media`] would have served.
///
/// The two are the same segment — the same cut points, the same duration, the same
/// byte range — so a text track stays addressable both ways at once: HLS asks for
/// the document while DASH asks for the packaged bytes.
pub(super) async fn text(
    op: &Operator,
    context: &RequestContext<()>,
    track_id: &str,
    file: &str,
) -> Result<Response, ServerError> {
    let (track, segment_options, range) = locate(op, context, track_id, file, ".vtt").await?;
    let bytes = track.read_range(op, &segment_options, range).await?;
    let fragments = wvtt::unpack(&bytes, track.timescale())?;
    let subtitle = fragmenter::merge(&fragments);

    Ok(([(CONTENT_TYPE, "text/vtt")], subtitle.write()).into_response())
}

/// The track `track_id` names and the byte range of the segment `file` names.
///
/// Segment start times are cumulative rather than stored, so the time in the
/// filename has to be one a segment begins at; a time inside one names nothing.
async fn locate(
    op: &Operator,
    context: &RequestContext<()>,
    track_id: &str,
    file: &str,
    extension: &str,
) -> Result<(Track, SegmentOptions, Range<u64>), ServerError> {
    let time = file
        .strip_suffix(extension)
        .and_then(|time| time.parse::<u64>().ok())
        .ok_or_else(|| ServerError::NotFound(file.to_string()))?;
    let (track, segment_options) = read_track(op, context, track_id).await?;

    let mut start_time = track.earliest_presentation_time();
    for segment in track.segments(&segment_options) {
        if start_time == time {
            return Ok((track, segment_options, segment.byte_range()));
        }
        start_time = start_time
            .checked_add(segment.raw_duration())
            .ok_or_else(|| ServerError::SegmentTimeOverflow(track_id.to_string()))?;
    }

    Err(ServerError::NotFound(format!(
        "segment {file} for track {track_id}"
    )))
}

/// The track `track_id` names, probed under the segment options this request asks
/// for. Those options are returned alongside it, since reading any of its bytes
/// has to package the track the same way the probe did.
async fn read_track(
    op: &Operator,
    context: &RequestContext<()>,
    track_id: &str,
) -> Result<(Track, SegmentOptions), ServerError> {
    let asset = context.read_asset(op).await?;
    let descriptor = asset
        .track(track_id)
        .ok_or_else(|| ServerError::NotFound(format!("track {track_id}")))?;
    let path = asset.track_path(descriptor);
    let segment_options = asset.segment_options.clone();
    let track = Track::probe(op, &path, Some(descriptor.kind.clone()), &segment_options).await?;

    Ok((track, segment_options))
}
