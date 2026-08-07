use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::filter::Filter;
use dyndo_core::track::Track;
use dyndo_dash::options::DashOptions;
use dyndo_hls::options::HlsOptions;
use opendal::Operator;
use serde::Serialize;

use super::context::RequestContext;
use super::filter;
use crate::error::ServerError;

const DASH_CONTENT_TYPE: &str = "application/dash+xml";
const HLS_CONTENT_TYPE: &str = "application/vnd.apple.mpegurl";

pub(super) async fn dash_manifest(
    op: &Operator,
    context: &RequestContext<DashOptions>,
    filter: Option<&Filter>,
) -> Result<Response, ServerError> {
    let asset = context.read_asset(op).await?;
    let tracks = Track::probe_all(op, &asset).await?;
    let (asset, tracks) = filter::apply(filter, asset, tracks)?;
    let mpd = dyndo_dash::builder::build_mpd(
        &asset,
        &tracks,
        &asset.segment_options,
        &context.transport_options,
    )?;
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let mut serializer = quick_xml::se::Serializer::new(&mut xml);
    serializer.indent(' ', 2);
    mpd.serialize(serializer)
        .map_err(|error| ServerError::Serialization(error.to_string()))?;
    Ok(([(CONTENT_TYPE, DASH_CONTENT_TYPE)], xml).into_response())
}

pub(super) async fn hls_master(
    op: &Operator,
    context: &RequestContext<HlsOptions>,
    filter: Option<&Filter>,
) -> Result<Response, ServerError> {
    let asset = context.read_asset(op).await?;
    let tracks = Track::probe_all(op, &asset).await?;
    let (asset, tracks) = filter::apply(filter, asset, tracks)?;
    let playlist = dyndo_hls::builder::build_master_playlist(
        &asset,
        &tracks,
        &asset.segment_options,
        &context.transport_options,
    )?;
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist.to_string()).into_response())
}

pub(super) async fn hls_media(
    op: &Operator,
    context: &RequestContext<HlsOptions>,
    track_id: &str,
) -> Result<Response, ServerError> {
    let asset = context.read_asset(op).await?;
    let descriptor = asset
        .track(track_id)
        .ok_or_else(|| ServerError::NotFound(format!("track {track_id}")))?;
    let playlist = dyndo_hls::builder::generate_media_playlist(
        op,
        &asset,
        descriptor,
        &context.transport_options,
    )
    .await?;
    Ok((
        [(CONTENT_TYPE, HLS_CONTENT_TYPE)],
        dyndo_hls::builder::serialize_media_playlist(&playlist),
    )
        .into_response())
}
