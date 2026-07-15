//! Manifest/playlist responders for every protocol. Each is a thin handler:
//! build the asset (or resolve one track) and hand it to the protocol's
//! generator. Media segments are protocol-agnostic and live in `super::segment`.

use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::asset::{AnyTrack, Asset};
use dyndo_core::model::AssetModel;
use opendal::Operator;

use super::find_source;
use crate::error::ServerError;

const DASH_CONTENT_TYPE: &str = "application/dash+xml";
const HLS_CONTENT_TYPE: &str = "application/vnd.apple.mpegurl";

/// `dash/index.mpd` — the asset's DASH manifest. It describes every
/// representation, so the whole asset is built (each track's header parsed).
pub(super) async fn dash_manifest(
    op: &Operator,
    asset_path: &str,
) -> Result<Response, ServerError> {
    let model = AssetModel::read(op, asset_path).await?;
    let asset = Asset::from_model(op, model, asset_path).await?;
    let xml = dyndo_core::dash::generate_mpd(&asset, true);
    Ok(([(CONTENT_TYPE, DASH_CONTENT_TYPE)], xml).into_response())
}

/// `hls/index.m3u8` — the asset's HLS multivariant playlist. Every rendition is
/// described, so the whole asset is built.
pub(super) async fn hls_master(op: &Operator, asset_path: &str) -> Result<Response, ServerError> {
    let model = AssetModel::read(op, asset_path).await?;
    let asset = Asset::from_model(op, model, asset_path).await?;
    let playlist = dyndo_core::hls::generate_master(&asset);
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist).into_response())
}

/// `hls/{repr}.m3u8` — one rendition's HLS media playlist.
pub(super) async fn hls_media(
    op: &Operator,
    asset_path: &str,
    repr: &str,
) -> Result<Response, ServerError> {
    let model = AssetModel::read(op, asset_path).await?;
    let track = AnyTrack::from_model(op, find_source(&model, repr)?, asset_path).await?;
    let playlist = dyndo_core::hls::generate_media(&track);
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist).into_response())
}
