//! HLS playlist generation for dyndo assets, built on `dyndo_core`.

mod build;

use dyndo_core::asset::Track;

/// Build the HLS media playlist for a single `track`: a VOD playlist with an
/// `EXT-X-MAP` init segment and one media segment per (sub)segment.
pub fn generate_media(track: &Track) -> String {
    build::build_media(track).to_string()
}
