use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::track::Track;
use dyndo_dash::options::DashOptions;
use dyndo_hls::options::HlsOptions;
use opendal::Operator;
use serde::Serialize;

use super::track_resolver::TrackResolver;
use crate::error::ServerError;

const DASH_CONTENT_TYPE: &str = "application/dash+xml";
const HLS_CONTENT_TYPE: &str = "application/vnd.apple.mpegurl";

pub(super) async fn dash(
    op: &Operator,
    asset: &AssetDescriptor,
    options: &DashOptions,
) -> Result<Response, ServerError> {
    let tracks = manifest_tracks(op, asset).await?;
    let mpd = dyndo_dash::generate_mpd(&tracks, &asset.segment_options, options)?;
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let mut serializer = quick_xml::se::Serializer::new(&mut xml);
    serializer.indent(' ', 2);
    mpd.serialize(serializer)
        .map_err(|error| ServerError::Serialization(error.to_string()))?;
    Ok(([(CONTENT_TYPE, DASH_CONTENT_TYPE)], xml).into_response())
}

pub(super) async fn hls_master(
    op: &Operator,
    asset: &AssetDescriptor,
    options: &HlsOptions,
) -> Result<Response, ServerError> {
    let tracks = manifest_tracks(op, asset).await?;
    let playlist = dyndo_hls::generate_master_playlist(&tracks, &asset.segment_options, options)?;
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist).into_response())
}

async fn manifest_tracks(
    op: &Operator,
    asset: &AssetDescriptor,
) -> Result<Vec<Track>, ServerError> {
    TrackResolver::new(op, asset).probe_all().await
}

pub(super) async fn hls_media(
    op: &Operator,
    asset: &AssetDescriptor,
    options: &HlsOptions,
    track_id: &str,
) -> Result<Response, ServerError> {
    let track = TrackResolver::new(op, asset).probe(track_id).await?;
    let playlist = dyndo_hls::generate_media_playlist(&track, &asset.segment_options, options)?;
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist).into_response())
}

pub(super) async fn hls_images(
    op: &Operator,
    asset: &AssetDescriptor,
    options: &HlsOptions,
    track_id: &str,
) -> Result<Response, ServerError> {
    let track = TrackResolver::new(op, asset).probe(track_id).await?;
    let playlist = dyndo_hls::generate_image_playlist(&track, options)?
        .ok_or_else(|| ServerError::NotFound("image playlist".to_string()))?;
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist).into_response())
}
