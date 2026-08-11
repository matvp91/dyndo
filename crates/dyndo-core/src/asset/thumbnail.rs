use serde::{Deserialize, Serialize};

/// The configuration of a thumbnail sprite sheet in an asset descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThumbnailTrackDescriptor {
    /// Identifier used to address the thumbnail track.
    pub id: String,
    /// Thumbnails per sprite row and column.
    pub tile_size: u32,
    /// Width of the complete sprite image, in pixels.
    pub width: u32,
    /// Milliseconds between adjacent thumbnails.
    pub step: u32,
}
