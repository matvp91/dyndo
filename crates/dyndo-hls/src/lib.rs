//! HLS playlist generation for dyndo assets.

mod master;
mod media;
pub mod options;
mod roles;

use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::filter::{Filter, FilterMatchedNothing};
use dyndo_core::probe::{ProbeError, probe_tracks};
use dyndo_core::track::Track;
use dyndo_core::track_descriptor::TrackDescriptor;
use hls_m3u8::{MasterPlaylist, MediaPlaylist};
use opendal::Operator;

use options::HlsOptions;

#[derive(Debug, thiserror::Error)]
pub enum HlsError {
    #[error(transparent)]
    Probe(#[from] ProbeError),
    #[error(transparent)]
    Playlist(#[from] hls_m3u8::Error),
    #[error("invalid video frame rate: {0}")]
    InvalidFrameRate(String),
    #[error(transparent)]
    Filter(#[from] FilterMatchedNothing),
}

/// Generates an HLS multivariant playlist for an asset.
///
/// `filter` narrows which of the asset's tracks the playlist describes; pass `None`
/// to describe all of them.
///
/// # Errors
///
/// Returns a [`HlsError`] when a track cannot be probed, the filter matches no
/// track, or the resulting playlist is invalid.
pub async fn generate_master_playlist(
    op: &Operator,
    asset: &AssetDescriptor,
    hls_options: &HlsOptions,
    filter: Option<&Filter>,
) -> Result<MasterPlaylist<'static>, HlsError> {
    let tracks = probe_tracks(op, asset).await?;
    let tracks = match filter {
        Some(filter) => filter.narrow(tracks, &asset.segment_options)?,
        None => tracks,
    };

    Ok(master::build_playlist(&tracks, &asset.segment_options, hls_options)?.build()?)
}

/// Generates the static HLS media playlist for one asset track.
///
/// # Errors
///
/// Returns a [`HlsError`] when the track cannot be probed or the resulting
/// playlist is invalid.
pub async fn generate_media_playlist(
    op: &Operator,
    asset: &AssetDescriptor,
    descriptor: &TrackDescriptor,
    hls_options: &HlsOptions,
) -> Result<MediaPlaylist<'static>, HlsError> {
    let path = asset.track_path(descriptor);
    let track = Track::probe(op, &path, Some(descriptor), &asset.segment_options).await?;

    Ok(media::build_playlist(&track, &asset.segment_options, hls_options)?.build()?)
}
