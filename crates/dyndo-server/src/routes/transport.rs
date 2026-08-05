use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_dash::options::DashOptions;
use dyndo_hls::options::HlsOptions;
use opendal::Operator;
use serde::Serialize;

use super::{RequestTransportOptions, read_asset};
use crate::error::ServerError;

const DASH_CONTENT_TYPE: &str = "application/dash+xml";
const HLS_CONTENT_TYPE: &str = "application/vnd.apple.mpegurl";

pub(super) async fn dash_manifest(
    op: &Operator,
    request_options: &RequestTransportOptions<DashOptions>,
) -> Result<Response, ServerError> {
    let asset = read_asset(op, &request_options.asset).await?;
    let mpd = dyndo_dash::builder::generate_mpd(
        op,
        &asset,
        &request_options.segment_options,
        request_options.transport_options.compact,
    )
    .await?;
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let mut serializer = quick_xml::se::Serializer::new(&mut xml);
    serializer.indent(' ', 2);
    mpd.serialize(serializer)
        .map_err(|error| ServerError::Serialization(error.to_string()))?;
    Ok(([(CONTENT_TYPE, DASH_CONTENT_TYPE)], xml).into_response())
}

pub(super) async fn hls_master(
    op: &Operator,
    request_options: &RequestTransportOptions<HlsOptions>,
) -> Result<Response, ServerError> {
    let asset = read_asset(op, &request_options.asset).await?;
    let playlist = dyndo_hls::builder::generate_master_playlist(
        op,
        &asset,
        &request_options.segment_options,
        &request_options.transport_options,
    )
    .await?;
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist.to_string()).into_response())
}

pub(super) async fn hls_media(
    op: &Operator,
    request_options: &RequestTransportOptions<HlsOptions>,
    track_id: &str,
) -> Result<Response, ServerError> {
    let asset = read_asset(op, &request_options.asset).await?;
    let descriptor = asset
        .track(track_id)
        .ok_or_else(|| ServerError::NotFound(format!("track {track_id}")))?;
    let playlist = dyndo_hls::builder::generate_media_playlist(
        op,
        &asset,
        descriptor,
        &request_options.segment_options,
        &request_options.transport_options,
    )
    .await?;
    Ok((
        [(CONTENT_TYPE, HLS_CONTENT_TYPE)],
        dyndo_hls::builder::serialize_media_playlist(&playlist),
    )
        .into_response())
}
