//! HLS playlist generation for dyndo assets.
//!
//! Playlists advertise the raw CMAF fragments — segment grouping
//! (`min_segment_length`, `segment_boundaries`) is not implemented in this
//! generation yet — and text and raw (non-CMAF) tracks are not advertised.

mod build;
mod group;

use crate::asset::Asset;
use crate::track::Track;

/// Build the HLS multivariant (master) playlist for `asset`: one
/// `EXT-X-STREAM-INF` per video variant and one `EXT-X-MEDIA` audio
/// rendition per audio track, grouped by audio codec. With no video, audio
/// tracks are the variants.
pub fn generate_master(asset: &Asset) -> String {
    build::build_master(asset).to_string()
}

/// Build the HLS media playlist for a single `track`: a VOD playlist with an
/// `EXT-X-MAP` init on the first segment and one media segment per raw CMAF
/// fragment, named by its running presentation time.
///
/// # Panics
/// If the track has not been probed, or is raw.
pub fn generate_media(track: &Track) -> String {
    build::build_media(track).to_string()
}
