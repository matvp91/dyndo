use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DashOptions {
    #[serde(default, alias = "c")]
    pub compact: bool,
    /// Split the manifest into a Period at each segment boundary.
    ///
    /// Off by default: a boundary only asks for a segment to start there, which
    /// says nothing about whether a client should treat what follows as a
    /// separate presentation.
    #[serde(default, alias = "mp")]
    pub multi_period: bool,
    /// Thumbnails per sprite row and column. Zero disables thumbnail output.
    #[serde(default, alias = "tts")]
    pub thumbnail_tile_size: u32,
    /// Milliseconds between adjacent thumbnails in a sprite.
    #[serde(default = "default_thumbnail_step", alias = "ts")]
    pub thumbnail_step: u32,
}

impl Default for DashOptions {
    fn default() -> Self {
        Self {
            compact: false,
            multi_period: false,
            thumbnail_tile_size: 0,
            thumbnail_step: default_thumbnail_step(),
        }
    }
}

const fn default_thumbnail_step() -> u32 {
    10_000
}
