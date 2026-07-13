//! HLS playlist generation for dyndo assets, built on `dyndo_core`.

#![deny(missing_docs)]

mod build;

use dyndo_core::asset::{Asset, Track};

/// Build the HLS media playlist for a single `track`: a VOD playlist with an
/// `EXT-X-MAP` init segment and one media segment per (sub)segment.
pub fn generate_media(track: &Track) -> String {
    build::build_media(track).to_string()
}

/// Build the HLS multivariant (master) playlist for `asset`: one
/// `EXT-X-STREAM-INF` per video variant and one `EXT-X-MEDIA` audio rendition
/// per audio track, grouped by audio codec.
pub fn generate_master(asset: &Asset) -> String {
    build::build_master(&asset.tracks).to_string()
}
