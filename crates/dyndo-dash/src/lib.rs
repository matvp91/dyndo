//! Static DASH manifest generation for dyndo assets.

mod adaptation_group;
mod builder;
mod compact;
pub mod options;
mod roles;
mod thumbnail;

use bytes::Bytes;
use dash_mpd::MPD;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::Track;
use opendal::Operator;
use options::DashOptions;

pub use thumbnail::ThumbnailError;

#[derive(Debug, thiserror::Error)]
pub enum DashError {
    #[error("tracks in an adaptation set are not segment-aligned")]
    SegmentAlignment,
    #[error(transparent)]
    Thumbnail(#[from] ThumbnailError),
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

/// Generates the JPEG sprite named by a DASH thumbnail segment start time.
///
/// The time is the `$Time$` substitution emitted by [`generate_mpd`], expressed
/// in milliseconds from the start of the video track.
///
/// # Errors
///
/// Returns an error when thumbnails are disabled or invalid, the track is not
/// video, the requested sprite is outside the track, or a frame cannot be read
/// or encoded.
pub async fn generate_thumbnail(
    op: &Operator,
    track: &Track,
    dash_options: &DashOptions,
    time: u64,
) -> Result<Bytes, ThumbnailError> {
    thumbnail::generate_jpeg(op, track, dash_options, time).await
}
