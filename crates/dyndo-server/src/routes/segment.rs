use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::image::Thumbnail;
use dyndo_core::reader::Reader;
use dyndo_core::text::Subtitle;
use opendal::Operator;

use super::track_resolver::{LocatedSegment, TrackResolver};
use crate::error::ServerError;

pub(super) async fn initialization(
    op: &Operator,
    asset: &AssetDescriptor,
    track_id: &str,
) -> Result<Response, ServerError> {
    let track = TrackResolver::new(op, asset).probe(track_id).await?;
    let bytes = Reader::new(op, &track, &asset.segment_options)
        .read_initialization()
        .await?;

    Ok(([(CONTENT_TYPE, track.kind().mime_type())], bytes).into_response())
}

pub(super) async fn media(
    op: &Operator,
    asset: &AssetDescriptor,
    track_id: &str,
    time: u64,
) -> Result<Response, ServerError> {
    let LocatedSegment { track, byte_range } = TrackResolver::new(op, asset)
        .locate_segment(track_id, time)
        .await?;
    let bytes = Reader::new(op, &track, &asset.segment_options)
        .read_range(byte_range)
        .await?;

    Ok(([(CONTENT_TYPE, track.kind().mime_type())], bytes).into_response())
}

pub(super) async fn text(
    op: &Operator,
    asset: &AssetDescriptor,
    track_id: &str,
    time: u64,
) -> Result<Response, ServerError> {
    let LocatedSegment { track, byte_range } = TrackResolver::new(op, asset)
        .locate_segment(track_id, time)
        .await?;
    let reader = Reader::new(op, &track, &asset.segment_options);
    let initialization = reader.read_initialization().await?;
    let segment = reader.read_range(byte_range).await?;
    let mut bytes = Vec::with_capacity(initialization.len() + segment.len());
    bytes.extend_from_slice(&initialization);
    bytes.extend_from_slice(&segment);
    let subtitle = Subtitle::from_wvtt(&bytes)?;

    Ok(([(CONTENT_TYPE, "text/vtt")], subtitle.to_vtt_text()).into_response())
}

/// Serves the thumbnail sprite named by the DASH `$Time$` substitution.
pub(super) async fn thumbnail(
    op: &Operator,
    asset: &AssetDescriptor,
    thumbnail_id: &str,
    time: u64,
) -> Result<Response, ServerError> {
    let descriptor = asset
        .find_thumbnail_by_id(thumbnail_id)
        .ok_or_else(|| ServerError::NotFound(format!("thumbnail {thumbnail_id}")))?;
    let tracks = TrackResolver::new(op, asset).probe_all().await?;
    let Some(thumbnail) = Thumbnail::new(descriptor, &tracks) else {
        return Err(ServerError::NotFound("thumbnail".to_string()));
    };
    let Some(bytes) = thumbnail.generate(op, time).await? else {
        return Err(ServerError::NotFound("thumbnail".to_string()));
    };

    Ok(([(CONTENT_TYPE, "image/jpeg")], bytes).into_response())
}
