use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::probe::probe_tracks;
use dyndo_core::track::Track;
use dyndo_dash::options::DashOptions;
use dyndo_hls::options::HlsOptions;
use opendal::Operator;
use serde::Serialize;

use super::context::RequestContext;
use super::filter::Filter;
use crate::error::ServerError;

const DASH_CONTENT_TYPE: &str = "application/dash+xml";
const HLS_CONTENT_TYPE: &str = "application/vnd.apple.mpegurl";

pub(super) async fn dash(
    op: &Operator,
    context: &RequestContext<DashOptions>,
    filter: Option<&Filter>,
) -> Result<Response, ServerError> {
    let asset = context.read_asset(op).await?;
    let tracks = probe_tracks(op, &asset).await?;
    let tracks = narrow(tracks, &asset.segment_options, filter)?;
    let mpd = dyndo_dash::generate_mpd(&tracks, &asset.segment_options, &context.manifest_options)?;
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
    let tracks = probe_tracks(op, &asset).await?;
    let tracks = narrow(tracks, &asset.segment_options, filter)?;
    let playlist = dyndo_hls::generate_master_playlist(
        &tracks,
        &asset.segment_options,
        &context.manifest_options,
    )?;
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist.to_string()).into_response())
}

fn narrow(
    tracks: Vec<Track>,
    options: &dyndo_core::segment_options::SegmentOptions,
    filter: Option<&Filter>,
) -> Result<Vec<Track>, ServerError> {
    match filter {
        Some(filter) => filter
            .narrow(tracks, options)
            .map_err(|_| ServerError::FilterMatchedNothing),
        None => Ok(tracks),
    }
}

pub(super) async fn hls_media(
    op: &Operator,
    context: &RequestContext<HlsOptions>,
    track_id: &str,
) -> Result<Response, ServerError> {
    let asset = context.read_asset(op).await?;
    let descriptor = asset
        .find_track_by_id(track_id)
        .ok_or_else(|| ServerError::NotFound(format!("track {track_id}")))?;
    let path = asset.track_path(descriptor);
    let track = Track::probe(op, &path, Some(descriptor), &asset.segment_options).await?;
    let playlist = dyndo_hls::generate_media_playlist(
        &track,
        &asset.segment_options,
        &context.manifest_options,
    )?;
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist.to_string()).into_response())
}
