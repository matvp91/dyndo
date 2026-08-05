use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use opendal::Operator;
use serde::Serialize;

use super::OutputParameters;
use crate::error::ServerError;

const DASH_CONTENT_TYPE: &str = "application/dash+xml";
const HLS_CONTENT_TYPE: &str = "application/vnd.apple.mpegurl";

pub(super) async fn dash_manifest(
    op: &Operator,
    parameters: &OutputParameters,
) -> Result<Response, ServerError> {
    let asset = parameters.read_asset(op).await?;
    let mpd = dyndo_dash::builder::generate_mpd(op, &asset).await?;
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let mut serializer = quick_xml::se::Serializer::new(&mut xml);
    serializer.indent(' ', 2);
    mpd.serialize(serializer)
        .map_err(|error| ServerError::Serialization(error.to_string()))?;
    Ok(([(CONTENT_TYPE, DASH_CONTENT_TYPE)], xml).into_response())
}

pub(super) async fn hls_master(
    op: &Operator,
    parameters: &OutputParameters,
) -> Result<Response, ServerError> {
    let asset = parameters.read_asset(op).await?;
    let playlist = dyndo_hls::builder::generate_master_playlist(op, &asset).await?;
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist.to_string()).into_response())
}

pub(super) async fn hls_media(
    op: &Operator,
    parameters: &OutputParameters,
    track_id: &str,
) -> Result<Response, ServerError> {
    let asset = parameters.read_asset(op).await?;
    let descriptor = asset
        .track(track_id)
        .ok_or_else(|| ServerError::NotFound(format!("track {track_id}")))?;
    let playlist = dyndo_hls::builder::generate_media_playlist(op, &asset, descriptor).await?;
    Ok((
        [(CONTENT_TYPE, HLS_CONTENT_TYPE)],
        dyndo_hls::builder::serialize_media_playlist(&playlist),
    )
        .into_response())
}
