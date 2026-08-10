//! Static DASH manifest generation for dyndo assets.

mod adaptation_group;
mod builder;
mod compact;
pub mod options;
mod roles;

use dash_mpd::MPD;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::Track;
use options::DashOptions;

#[derive(Debug, thiserror::Error)]
pub enum DashError {
    #[error("tracks in an adaptation set are not segment-aligned")]
    SegmentAlignment,
}

/// Generates a static DASH media presentation description for an asset.
///
/// # Errors
///
/// Returns a [`DashError`] when tracks grouped into an AdaptationSet are not
/// segment-aligned.
pub fn generate_mpd(
    tracks: &[Track],
    segment_options: &SegmentOptions,
    dash_options: &DashOptions,
) -> Result<MPD, DashError> {
    let mut mpd = builder::build_mpd(tracks, segment_options, dash_options)?;
    if dash_options.compact {
        compact::compact(&mut mpd);
    }

    Ok(mpd)
}
