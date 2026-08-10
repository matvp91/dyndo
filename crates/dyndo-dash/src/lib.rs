//! Static DASH manifest generation for dyndo assets.

mod adaptation_group;
mod builder;
mod compact;
pub mod options;
mod roles;

use dash_mpd::MPD;
use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::filter::{Filter, FilterMatchedNothing};
use dyndo_core::probe::{ProbeError, probe_tracks};
use opendal::Operator;

use options::DashOptions;

#[derive(Debug, thiserror::Error)]
pub enum DashError {
    #[error(transparent)]
    Probe(#[from] ProbeError),
    #[error("tracks in an adaptation set are not segment-aligned")]
    SegmentAlignment,
    #[error(transparent)]
    Filter(#[from] FilterMatchedNothing),
}

/// Generates a static DASH media presentation description for an asset.
///
/// `filter` narrows which of the asset's tracks the manifest describes; pass `None`
/// to describe all of them.
///
/// # Errors
///
/// Returns a [`DashError`] when a track cannot be probed, the filter matches no
/// track, or tracks grouped into an AdaptationSet are not segment-aligned.
pub async fn generate_mpd(
    op: &Operator,
    asset: &AssetDescriptor,
    dash_options: &DashOptions,
    filter: Option<&Filter>,
) -> Result<MPD, DashError> {
    let tracks = probe_tracks(op, asset).await?;
    let tracks = match filter {
        Some(filter) => filter.narrow(tracks, &asset.segment_options)?,
        None => tracks,
    };
    let mut mpd = builder::build_mpd(&tracks, &asset.segment_options, dash_options)?;
    if dash_options.compact {
        compact::compact(&mut mpd);
    }

    Ok(mpd)
}
