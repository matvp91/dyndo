use std::ops::Range;

use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::packaging::UnpackagedMedia;
use dyndo_core::packaging::wvtt::{WvttSample, WvttUnpackager};
use dyndo_core::reader::Reader;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::served_segment::ServedSegment;
use dyndo_core::text::{Cue, Subtitle};
use dyndo_core::time::Time;
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

/// Serves the segment starting at `time` as the packaged CMAF bytes it is stored as.
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
    let reader = Reader::new(op, &track, &segment_options);
    let initialization = reader.read_initialization().await?;
    let segment = reader.read_range(range).await?;
    let mut bytes = Vec::with_capacity(initialization.len() + segment.len());
    bytes.extend_from_slice(&initialization);
    bytes.extend_from_slice(&segment);
    let media = WvttUnpackager::new().unpackage(&bytes)?;
    let subtitle = subtitle(&media)?;

    Ok(([(CONTENT_TYPE, "text/vtt")], subtitle.to_vtt_text()).into_response())
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
        .find_track_by_id(track_id)
        .ok_or_else(|| ServerError::NotFound(format!("track {track_id}")))?;
    let path = asset.track_path(descriptor);
    let segment_options = asset.segment_options.clone();
    let track = Track::probe(op, &path, Some(descriptor), &segment_options).await?;

    Ok((track, segment_options))
}

fn subtitle(media: &UnpackagedMedia<WvttSample>) -> Result<Subtitle, ServerError> {
    let mut cues: Vec<Cue> = Vec::new();
    let mut open: Vec<usize> = Vec::new();

    for segment in media.segments() {
        let mut start = segment.base_decode_time();
        for sample in segment.samples() {
            let end = start
                .checked_add(u64::from(sample.duration()))
                .ok_or(ServerError::SubtitleTimeOverflow(start))?;
            let start_ms = timestamp(start, media.timescale())?;
            let end_ms = timestamp(end, media.timescale())?;
            let mut still_open = Vec::with_capacity(sample.payload().cues().len());

            for text in sample.payload().cues() {
                let continued = open
                    .iter()
                    .copied()
                    .find(|&index| cues[index].end == start_ms && cues[index].text == *text);
                match continued {
                    Some(index) => {
                        cues[index].end = end_ms;
                        still_open.push(index);
                    }
                    None => {
                        cues.push(Cue {
                            start: start_ms,
                            end: end_ms,
                            text: text.clone(),
                        });
                        still_open.push(cues.len() - 1);
                    }
                }
            }

            open = still_open;
            start = end;
        }
    }

    cues.sort_by_key(|cue| (cue.start, cue.end));
    Ok(Subtitle { cues })
}

fn timestamp(time: u64, timescale: u32) -> Result<u32, ServerError> {
    let milliseconds = Time::milliseconds(time, timescale);
    u32::try_from(milliseconds).map_err(|_| ServerError::SubtitleTimeOverflow(milliseconds))
}
