//! Static DASH manifest generation for dyndo assets.

mod adaptation_group;
mod builder;
mod compact;
mod multi_period;
pub mod options;
mod roles;
mod thumbnail;

use dash_mpd::MPD;
use dyndo_core::asset::ResolvedAsset;
use dyndo_core::track::CmafRepresentationError;
use options::DashOptions;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum DashError {
    #[error(transparent)]
    CmafRepresentation(#[from] CmafRepresentationError),
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
    #[error("manifest serialization failed: {0}")]
    Serialization(String),
}

/// Generates a static DASH media presentation description for an asset.
///
/// # Errors
///
/// Returns a [`DashError`] when a track cannot provide CMAF metadata,
/// multi-period transformation fails, or XML serialization fails.
pub async fn generate_mpd(
    asset: &ResolvedAsset,
    min_length: u32,
    text_length: u32,
    dash_options: &DashOptions,
) -> Result<String, DashError> {
    let tracks = asset.cmaf_representations(text_length).await?;
    let thumbnails: Vec<_> = asset.thumbnails().cloned().collect();
    let mut mpd = builder::build_mpd(&tracks, &thumbnails, min_length, asset.boundaries());
    if dash_options.multi_period {
        multi_period::split(&mut mpd, asset.boundaries())?;
    }
    if dash_options.compact {
        compact::compact(&mut mpd);
    }

    serialize(&mpd)
}

fn serialize(mpd: &MPD) -> Result<String, DashError> {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let mut serializer = quick_xml::se::Serializer::new(&mut xml);
    serializer.indent(' ', 2);
    mpd.serialize(serializer)
        .map_err(|error| DashError::Serialization(error.to_string()))?;
    Ok(xml)
}
