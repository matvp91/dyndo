//! HLS playlist generation for dyndo assets.
//!
//! Media playlists advertise the served segments — the raw CMAF fragments
//! grouped under the asset's `min_segment_length` and `segment_boundaries`
//! — and text and raw (non-CMAF) tracks are not advertised.

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
/// `EXT-X-MAP` init on the first segment and one media segment per served
/// (sub)segment, named by its running presentation time. `boundaries_ms` and
/// `min_length_ms` are the asset's grouping pair — the segment route must
/// serve under the same pair or advertised times will not resolve.
///
/// A raw track yields a playlist with no segments: a raw file has no
/// segment map of its own, and raw tracks are never advertised in the
/// master playlist.
///
/// # Panics
/// If the track has not been probed.
pub fn generate_media(track: &Track, boundaries_ms: &[u64], min_length_ms: u64) -> String {
    build::build_media(track, boundaries_ms, min_length_ms).to_string()
}
