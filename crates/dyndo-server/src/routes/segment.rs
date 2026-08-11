use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::asset::AssetDescriptor;
use dyndo_core::reader::Reader;
use dyndo_core::track::synthetic::resolve_synthetic_tracks;
use opendal::Operator;

use super::track_resolver::{LocatedSegment, ResolvedTrack, TrackResolver};
use crate::error::ServerError;

pub(super) async fn initialization(
    op: &Operator,
    asset: &AssetDescriptor,
    track_id: &str,
) -> Result<Response, ServerError> {
    let track = TrackResolver::new(op, asset).resolve(track_id).await?;
    let bytes = read_range(op, &track, track.cmaf().init_segment().byte_range()).await?;

    Ok(([(CONTENT_TYPE, track.cmaf().kind().mime_type())], bytes).into_response())
}

pub(super) async fn media(
    op: &Operator,
    asset: &AssetDescriptor,
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
    asset: &AssetDescriptor,
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
    track: &ResolvedTrack,
    range: std::ops::Range<u64>,
) -> Result<bytes::Bytes, ServerError> {
    match track.packaged() {
        Some(track) => track
            .read(range)
            .ok_or_else(|| ServerError::NotFound("packaged VTT byte range".to_string())),
        None => Ok(Reader::new(op).read_range(track.cmaf(), range).await?),
    }
}

/// Serves the thumbnail sprite named by the DASH `$Time$` substitution.
pub(super) async fn thumbnail(
    op: &Operator,
    asset: &AssetDescriptor,
    thumbnail_id: &str,
    time: u64,
) -> Result<Response, ServerError> {
    let descriptor = asset
        .find_thumbnail_track_by_id(thumbnail_id)
        .ok_or_else(|| ServerError::NotFound(format!("thumbnail {thumbnail_id}")))?;
    let source_tracks = TrackResolver::new(op, asset).probe_all().await?;
    let thumbnail = resolve_synthetic_tracks(asset, &source_tracks)
        .into_iter()
        .find(|track| track.id() == descriptor.id);
    let Some(thumbnail) = thumbnail else {
        return Err(ServerError::NotFound("thumbnail".to_string()));
    };
    let Some(bytes) = thumbnail.jpeg(op, time).await? else {
        return Err(ServerError::NotFound("thumbnail".to_string()));
    };

    Ok(([(CONTENT_TYPE, "image/jpeg")], bytes).into_response())
}
