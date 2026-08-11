//! HLS playlist generation for dyndo assets.

mod master;
mod media;
pub mod options;
mod renditions;
mod roles;

use std::io;

use dyndo_core::asset::ResolvedAsset;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::cmaf::CmafKind;
use dyndo_core::track::thumbnail::ResolvedThumbnailTrack;
use dyndo_core::track::{CmafRepresentationError, ResolvedTrack};
use options::HlsOptions;

#[derive(Debug, thiserror::Error)]
pub enum HlsError {
    #[error(transparent)]
    CmafRepresentation(#[from] CmafRepresentationError),
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
/// Returns a [`HlsError`] when a track cannot provide a CMAF representation or
/// the resulting playlist is invalid.
pub async fn generate_master_playlist(
    asset: &ResolvedAsset,
    hls_options: &HlsOptions,
) -> Result<String, HlsError> {
    let tracks = asset.cmaf_representations().await?;
    let thumbnails: Vec<_> = asset.thumbnails().cloned().collect();
    let mut hls_options = *hls_options;
    hls_options.wvtt |= asset
        .tracks()
        .iter()
        .filter_map(ResolvedTrack::cmaf)
        .any(|track| matches!(track.kind(), CmafKind::Text(_)));
    let playlist =
        master::build_playlist(&tracks, &thumbnails, asset.segment_options(), &hls_options)?;
    serialize(|output| playlist.write_to(output))
}

/// Generates the static HLS media playlist for one asset track.
///
/// # Errors
///
/// Returns a [`HlsError`] when the track cannot provide a CMAF representation
/// or the resulting playlist is invalid.
pub async fn generate_media_playlist(
    track: &ResolvedTrack,
    segment_options: &SegmentOptions,
    hls_options: &HlsOptions,
) -> Result<String, HlsError> {
    let cmaf = track.cmaf_representation(segment_options).await?;
    let mut hls_options = *hls_options;
    hls_options.wvtt |= track.timed_text().is_none();
    let playlist = media::build_playlist(&cmaf, segment_options, &hls_options);
    serialize(|output| playlist.write_to(output))
}

/// Generates the image media playlist for one thumbnail track.
pub fn generate_image_playlist(thumbnail: &ResolvedThumbnailTrack) -> Result<String, HlsError> {
    let playlist = media::build_image_playlist(thumbnail);
    serialize(|output| playlist.write_to(output))
}

fn serialize(write: impl FnOnce(&mut Vec<u8>) -> io::Result<()>) -> Result<String, HlsError> {
    let mut output = Vec::new();
    write(&mut output)?;
    Ok(String::from_utf8(output)?)
}
