//! Static DASH manifest generation for dyndo assets.

mod adaptation_group;
mod builder;
mod compact;
mod multi_period;
pub mod options;
mod roles;
mod thumbnail;

use dash_mpd::MPD;
use dyndo_core::image::Thumbnail;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::Track;
use options::DashOptions;

#[derive(Debug, thiserror::Error)]
pub enum DashError {
    #[error("tracks in an adaptation set are not segment-aligned")]
    SegmentAlignment,
    #[error("multi-period splitting requires at most one MPD Period")]
    MultiPeriodSource,
    #[error(
        "multi-period splitting requires an MPD presentation duration that fits in milliseconds"
    )]
    MultiPeriodDuration,
    #[error(
        "multi-period splitting requires every representation to have a SegmentTemplate timeline"
    )]
    MultiPeriodTemplate,
    #[error("multi-period splitting cannot expand the MPD SegmentTimeline")]
    MultiPeriodTimeline,
}

/// Generates a static DASH media presentation description for an asset.
///
/// # Errors
///
/// Returns a [`DashError`] when tracks grouped into an AdaptationSet are not
/// segment-aligned.
pub fn generate_mpd(
    tracks: &[Track],
    thumbnails: &[Thumbnail<'_>],
    segment_options: &SegmentOptions,
    dash_options: &DashOptions,
) -> Result<MPD, DashError> {
    let mut mpd = builder::build_mpd(tracks, thumbnails, segment_options)?;
    if dash_options.multi_period {
        multi_period::split(&mut mpd, &segment_options.boundaries)?;
    }
    if dash_options.compact {
        compact::compact(&mut mpd);
    }

    Ok(mpd)
}
