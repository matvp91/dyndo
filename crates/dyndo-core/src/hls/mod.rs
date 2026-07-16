//! HLS playlist generation for dyndo assets.

mod build;

use crate::asset::{Asset, Track};

/// Build the HLS media playlist for a single `track`: a VOD playlist with an
/// `EXT-X-MAP` init segment and one media segment per served (sub)segment —
/// pass the same grouping policy the segment route uses.
pub fn generate_media(
    track: &impl Track,
    segment_boundaries_ms: &[u64],
    min_segment_length_ms: u64,
) -> String {
    build::build_media(track, segment_boundaries_ms, min_segment_length_ms).to_string()
}

/// Build the HLS multivariant (master) playlist for `asset`: one
/// `EXT-X-STREAM-INF` per video variant and one `EXT-X-MEDIA` audio rendition
/// per audio track, grouped by audio codec.
pub fn generate_master(asset: &Asset) -> String {
    build::build_master(&asset.video_tracks, &asset.audio_tracks, &asset.text_tracks).to_string()
}
