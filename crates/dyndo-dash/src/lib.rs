//! Static DASH manifest generation for dyndo assets.

mod builder;
mod compact;
pub mod options;
mod roles;
mod split;

use dyndo_core::{asset::Asset, mp4_readable::Mp4ReadableError, text::SubtitleReadError};
use thiserror::Error;

use crate::options::DashOptions;

/// Errors returned while generating a DASH manifest.
#[derive(Debug, Error)]
pub enum DashError {
    /// A CMAF segment index could not be read.
    #[error(transparent)]
    Mp4(#[from] Mp4ReadableError),
    /// A sidecar subtitle could not be loaded.
    #[error(transparent)]
    Subtitle(#[from] SubtitleReadError),
    /// The configured text segment duration was too short.
    #[error("text segment duration must be at least one millisecond")]
    TextSegmentDuration,
    /// Multi-period splitting requires at most one source Period.
    #[error("multi-period splitting requires at most one MPD Period")]
    MultiPeriodSource,
    /// Multi-period splitting requires a presentation duration.
    #[error("multi-period splitting requires an MPD presentation duration")]
    MultiPeriodDuration,
    /// Multi-period splitting requires a timeline-based SegmentTemplate.
    #[error(
        "multi-period splitting requires every representation to have a SegmentTemplate timeline"
    )]
    MultiPeriodTemplate,
    /// A SegmentTimeline could not be expanded safely.
    #[error("multi-period splitting cannot expand the MPD SegmentTimeline")]
    MultiPeriodTimeline,
    /// The MPD could not be serialized.
    #[error("manifest serialization failed: {0}")]
    Serialization(String),
}

/// Generates a static DASH MPD for an asset.
///
/// # Errors
///
/// Returns [`DashError`] when a source track cannot be read, a sidecar text
/// track cannot be parsed, or the resulting MPD cannot be serialized.
pub async fn generate_mpd(asset: &Asset, options: DashOptions) -> Result<String, DashError> {
    builder::generate(asset, options).await
}
