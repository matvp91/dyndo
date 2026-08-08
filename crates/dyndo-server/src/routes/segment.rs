use std::ops::Range;

use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::segment::{self, SegmentOptions};
use dyndo_core::track::Track;
use dyndo_text::{fragmenter, vtt, wvtt};
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

    Ok(([(CONTENT_TYPE, track.kind().mime_type())], bytes).into_response())
}

/// Serves the segment starting at `time` as the packaged CMAF bytes it is stored as.
pub(super) async fn media(
    op: &Operator,
    context: &RequestContext<()>,
    track_id: &str,
    time: u64,
) -> Result<Response, ServerError> {
    let (track, segment_options, range) = locate(op, context, track_id, time).await?;
    let bytes = track.read_range(op, &segment_options, range).await?;

    Ok(([(CONTENT_TYPE, track.kind().mime_type())], bytes).into_response())
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
    time: u64,
) -> Result<Response, ServerError> {
    let (track, segment_options, range) = locate(op, context, track_id, time).await?;
    let bytes = track.read_range(op, &segment_options, range).await?;
    let fragments = wvtt::unpack(&bytes, track.timescale())?;
    let subtitle = fragmenter::merge(&fragments);

    Ok(([(CONTENT_TYPE, "text/vtt")], vtt::write(&subtitle)).into_response())
}

/// The track `track_id` names and the byte range of the segment starting at `time`.
///
/// `time` has to be one a segment begins at, since that is what a manifest addresses
/// them by; a time inside one names nothing.
async fn locate(
    op: &Operator,
    context: &RequestContext<()>,
    track_id: &str,
    time: u64,
) -> Result<(Track, SegmentOptions, Range<u64>), ServerError> {
    let (track, segment_options) = read_track(op, context, track_id).await?;
    let segment = segment::segments(&track, &segment_options)
        .into_iter()
        .find(|segment| segment.raw_start() == time)
        .ok_or_else(|| ServerError::NotFound(format!("segment {time} for track {track_id}")))?;

    Ok((track, segment_options, segment.byte_range()))
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
    let track = Track::probe(op, &path, Some(descriptor), &segment_options).await?;

    Ok((track, segment_options))
}
