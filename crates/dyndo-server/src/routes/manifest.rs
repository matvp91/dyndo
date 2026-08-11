use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::asset::Asset;
use dyndo_core::track::ResolvedSourceTrack;
use dyndo_core::track::cmaf::ResolvedCmafTrack;
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
    source_asset: &Asset,
    asset: &Asset,
    options: &DashOptions,
) -> Result<Response, ServerError> {
    let source_tracks = TrackResolver::new(op, source_asset)
        .resolve_sources()
        .await?;
    let thumbnails = asset.resolve_thumbnails(&source_tracks);
    let tracks = filtered_tracks(source_tracks, asset).await?;
    let mpd = dyndo_dash::generate_mpd(&tracks, &thumbnails, &asset.segment_options, options)?;
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let mut serializer = quick_xml::se::Serializer::new(&mut xml);
    serializer.indent(' ', 2);
    mpd.serialize(serializer)
        .map_err(|error| ServerError::Serialization(error.to_string()))?;
    Ok(([(CONTENT_TYPE, DASH_CONTENT_TYPE)], xml).into_response())
}

pub(super) async fn hls_master(
    op: &Operator,
    source_asset: &Asset,
    asset: &Asset,
    options: &HlsOptions,
) -> Result<Response, ServerError> {
    let source_tracks = TrackResolver::new(op, source_asset)
        .resolve_sources()
        .await?;
    let thumbnails = asset.resolve_thumbnails(&source_tracks);
    let mut hls_options = *options;
    hls_options.wvtt |= source_tracks
        .iter()
        .filter_map(ResolvedSourceTrack::cmaf)
        .any(|track| track.kind().is_text());
    let tracks = filtered_tracks(source_tracks, asset).await?;
    let playlist = dyndo_hls::generate_master_playlist(
        &tracks,
        &thumbnails,
        &asset.segment_options,
        &hls_options,
    )?;
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist).into_response())
}

pub(super) async fn hls_media(
    op: &Operator,
    asset: &Asset,
    options: &HlsOptions,
    track_id: &str,
) -> Result<Response, ServerError> {
    let track = TrackResolver::new(op, asset).resolve(track_id).await?;
    let mut hls_options = *options;
    hls_options.wvtt |= !track.is_web_vtt();
    let playlist =
        dyndo_hls::generate_media_playlist(track.cmaf(), &asset.segment_options, &hls_options)?;
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist).into_response())
}

pub(super) async fn hls_images(
    op: &Operator,
    source_asset: &Asset,
    asset: &Asset,
    thumbnail_id: &str,
) -> Result<Response, ServerError> {
    let configured = asset
        .find_thumbnail_track_by_id(thumbnail_id)
        .ok_or_else(|| ServerError::NotFound(format!("thumbnail {thumbnail_id}")))?;
    let source_tracks = TrackResolver::new(op, source_asset)
        .resolve_sources()
        .await?;
    let thumbnail = asset
        .resolve_thumbnails(&source_tracks)
        .into_iter()
        .find(|track| track.id() == configured.id)
        .ok_or_else(|| ServerError::NotFound("thumbnail".to_string()))?;
    let playlist = dyndo_hls::generate_image_playlist(&thumbnail)?;
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist).into_response())
}

async fn filtered_tracks(
    tracks: Vec<ResolvedSourceTrack>,
    asset: &Asset,
) -> Result<Vec<ResolvedCmafTrack>, ServerError> {
    let mut resolved = Vec::new();
    for track in tracks {
        if asset.find_source_track_by_id(track.id()).is_none() {
            continue;
        }
        match track {
            ResolvedSourceTrack::Cmaf(track) => resolved.push(track),
            ResolvedSourceTrack::TimedText(track) => {
                resolved.push(track.package_wvtt(&asset.segment_options).await?)
            }
        }
    }
    Ok(resolved)
}
