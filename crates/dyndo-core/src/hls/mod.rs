//! HLS playlist generation for dyndo assets.

mod build;

use crate::asset::{Asset, Track};

/// Build the HLS media playlist for a single `track`: a VOD playlist with an
/// `EXT-X-MAP` init segment and one media segment per (sub)segment.
pub fn generate_media<T: Track>(track: &T) -> String {
    build::build_media(track).to_string()
}

/// Build the HLS multivariant (master) playlist for `asset`: one
/// `EXT-X-STREAM-INF` per video variant and one `EXT-X-MEDIA` audio rendition
/// per audio track, grouped by audio codec.
pub fn generate_master(asset: &Asset) -> String {
    build::build_master(&asset.video_tracks, &asset.audio_tracks).to_string()
}
