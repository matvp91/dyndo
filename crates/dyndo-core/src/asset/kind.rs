use language_tags::LanguageTag;
use serde::{Deserialize, Serialize};

use crate::role::Role;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoKind {
    pub width: u32,
    pub height: u32,
    pub frame_rate: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioKind {
    pub sample_rate: u32,
    pub channels: u16,
    #[serde(default = "undetermined_language")]
    pub language: LanguageTag,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextKind {
    #[serde(default = "undetermined_language")]
    pub language: LanguageTag,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
}

/// The configuration of a thumbnail sprite sheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThumbnailKind {
    /// Thumbnails per sprite row and column.
    pub tile_size: u32,
    /// Width of the complete sprite image, in pixels.
    pub width: u32,
    /// Milliseconds between adjacent thumbnails.
    pub step: u32,
}

pub(crate) fn undetermined_language() -> LanguageTag {
    "und".parse().expect("und is a well-formed language tag")
}
