//! Static DASH manifest generation for dyndo assets.

mod adaptation_set_group;
mod builder;
mod compact;
pub mod options;
mod roles;

use dash_mpd::MPD;
use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::filter::Filter;
use dyndo_core::probe::probe_tracks;
use opendal::Operator;

pub use builder::DashError;
use options::DashOptions;

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
