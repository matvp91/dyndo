use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::asset::ResolvedAsset;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::ResolvedTrack;
use dyndo_dash::options::DashOptions;
use dyndo_hls::options::HlsOptions;

use crate::error::ServerError;

const DASH_CONTENT_TYPE: &str = "application/dash+xml";
const HLS_CONTENT_TYPE: &str = "application/vnd.apple.mpegurl";

pub(super) async fn dash(
    asset: &ResolvedAsset,
    options: &DashOptions,
) -> Result<Response, ServerError> {
    let xml = dyndo_dash::generate_mpd(asset, options).await?;
    Ok(([(CONTENT_TYPE, DASH_CONTENT_TYPE)], xml).into_response())
}

pub(super) async fn hls_master(
    asset: &ResolvedAsset,
    options: &HlsOptions,
) -> Result<Response, ServerError> {
    let playlist = dyndo_hls::generate_master_playlist(asset, options).await?;
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist).into_response())
}

pub(super) async fn hls_media(
    track: &ResolvedTrack,
    segment_options: &SegmentOptions,
    options: &HlsOptions,
) -> Result<Response, ServerError> {
    let playlist = dyndo_hls::generate_media_playlist(track, segment_options, options).await?;
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist).into_response())
}

pub(super) async fn hls_images(track: &ResolvedTrack) -> Result<Response, ServerError> {
    let thumbnail = track
        .thumbnail()
        .ok_or_else(|| ServerError::NotFound(format!("thumbnail {}", track.id())))?;
    let playlist = dyndo_hls::generate_image_playlist(thumbnail)?;
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist).into_response())
}
