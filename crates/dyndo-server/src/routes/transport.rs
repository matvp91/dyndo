//! Manifest/playlist responders for every protocol. Each is a thin handler:
//! read the asset (every track's header probed) and hand it to the
//! protocol's generator. Media segments are protocol-agnostic and live in
//! `super::segment`.

use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::asset::Asset;
use opendal::Operator;

use super::find_track;
use crate::error::ServerError;

const DASH_CONTENT_TYPE: &str = "application/dash+xml";
const HLS_CONTENT_TYPE: &str = "application/vnd.apple.mpegurl";

/// `dash/index.mpd` — the asset's DASH manifest. It describes every
/// representation, so the whole asset is read.
pub(super) async fn dash_manifest(
    op: &Operator,
    asset_path: &str,
) -> Result<Response, ServerError> {
    let asset = Asset::read(op, asset_path).await?;
    let xml = dyndo_core::dash::generate_mpd(&asset, true);
    Ok(([(CONTENT_TYPE, DASH_CONTENT_TYPE)], xml).into_response())
}

/// `hls/index.m3u8` — the asset's HLS multivariant playlist. Every rendition
/// is described, so the whole asset is read.
pub(super) async fn hls_master(op: &Operator, asset_path: &str) -> Result<Response, ServerError> {
    let asset = Asset::read(op, asset_path).await?;
    let playlist = dyndo_core::hls::generate_master(&asset);
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist).into_response())
}

/// `hls/{repr}.m3u8` — one rendition's HLS media playlist.
pub(super) async fn hls_media(
    op: &Operator,
    asset_path: &str,
    repr: &str,
) -> Result<Response, ServerError> {
    let asset = Asset::read(op, asset_path).await?;
    let track = find_track(&asset, repr)?;
    // Raw tracks resolve by id but are never advertised in the master
    // playlist — a media playlist for one would be empty, so answer 404.
    if track.is_raw() {
        return Err(ServerError::NotFound(format!(
            "no media playlist for {repr}"
        )));
    }
    let playlist = dyndo_core::hls::generate_media(
        track,
        &asset.segment_boundaries_ms,
        asset.min_segment_length_ms,
    );
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist).into_response())
}
