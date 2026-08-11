//! HLS playlist generation for dyndo assets.

mod master;
mod media;
pub mod options;
mod renditions;
mod roles;

use std::io;

use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::cmaf::CmafTrack;
use dyndo_core::track::thumbnail::ThumbnailTrack;
use options::HlsOptions;

#[derive(Debug, thiserror::Error)]
pub enum HlsError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("invalid video frame rate: {0}")]
    InvalidFrameRate(String),
}

/// Generates an HLS multivariant playlist for an asset.
///
/// # Errors
///
/// Returns a [`HlsError`] when the resulting playlist is invalid.
pub fn generate_master_playlist(
    tracks: &[CmafTrack],
    thumbnails: &[ThumbnailTrack],
    segment_options: &SegmentOptions,
    hls_options: &HlsOptions,
) -> Result<String, HlsError> {
    let playlist = master::build_playlist(tracks, thumbnails, segment_options, hls_options)?;
    serialize(|output| playlist.write_to(output))
}

/// Generates the static HLS media playlist for one asset track.
///
/// # Errors
///
/// Returns a [`HlsError`] when the resulting playlist is invalid.
pub fn generate_media_playlist(
    track: &CmafTrack,
    segment_options: &SegmentOptions,
    hls_options: &HlsOptions,
) -> Result<String, HlsError> {
    let playlist = media::build_playlist(track, segment_options, hls_options);
    serialize(|output| playlist.write_to(output))
}

/// Generates the image media playlist for one thumbnail track.
pub fn generate_image_playlist(thumbnail: &ThumbnailTrack) -> Result<String, HlsError> {
    let playlist = media::build_image_playlist(thumbnail);
    serialize(|output| playlist.write_to(output))
}

fn serialize(write: impl FnOnce(&mut Vec<u8>) -> io::Result<()>) -> Result<String, HlsError> {
    let mut output = Vec::new();
    write(&mut output)?;
    Ok(String::from_utf8(output)?)
}

fn media_resource_name(track: &CmafTrack) -> String {
    format!("{}_{}", track.kind().content_type(), track.id())
}

fn image_resource_name(thumbnail: &ThumbnailTrack) -> String {
    format!("image_{}", thumbnail.id())
}
