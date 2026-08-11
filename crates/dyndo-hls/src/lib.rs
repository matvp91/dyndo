//! HLS playlist generation for dyndo assets.

mod image;
mod master;
mod media;
pub mod options;
mod roles;

use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::Track;
use hls_m3u8::{MasterPlaylist, MediaPlaylist};
use options::HlsOptions;

#[derive(Debug, thiserror::Error)]
pub enum HlsError {
    #[error(transparent)]
    Playlist(#[from] hls_m3u8::Error),
    #[error("invalid video frame rate: {0}")]
    InvalidFrameRate(String),
}

/// Generates an HLS multivariant playlist for an asset.
///
/// # Errors
///
/// Returns a [`HlsError`] when the resulting playlist is invalid.
pub fn generate_master_playlist(
    tracks: &[Track],
    segment_options: &SegmentOptions,
    hls_options: &HlsOptions,
) -> Result<MasterPlaylist<'static>, HlsError> {
    Ok(master::build_playlist(tracks, segment_options, hls_options)?.build()?)
}

/// Generates the static HLS media playlist for one asset track.
///
/// # Errors
///
/// Returns a [`HlsError`] when the resulting playlist is invalid.
pub fn generate_media_playlist(
    track: &Track,
    segment_options: &SegmentOptions,
    hls_options: &HlsOptions,
) -> Result<MediaPlaylist<'static>, HlsError> {
    Ok(media::build_playlist(track, segment_options, hls_options)?.build()?)
}

/// Returns the image media playlist for a video track when thumbnails are enabled.
pub fn generate_image_playlist(track: &Track, hls_options: &HlsOptions) -> Option<String> {
    image::build_playlist(track, hls_options)
}

fn media_resource_name(track: &Track) -> String {
    format!("{}_{}", track.kind().content_type(), track.id())
}

fn image_resource_name(track: &Track) -> String {
    format!("image_{}", track.id())
}
