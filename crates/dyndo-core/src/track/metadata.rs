use language_tags::LanguageTag;
use serde::{Deserialize, Serialize};

use crate::role::Role;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoMetadata {
    pub width: u32,
    pub height: u32,
    pub frame_rate: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioMetadata {
    pub sample_rate: u32,
    pub channels: u16,
    #[serde(default = "undetermined_language")]
    pub language: LanguageTag,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
}

/// Language and presentation metadata shared by text representations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextMetadata {
    #[serde(default = "undetermined_language")]
    pub language: LanguageTag,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
}

impl Default for TextMetadata {
    fn default() -> Self {
        Self {
            language: undetermined_language(),
            role: None,
        }
    }
}

pub(crate) fn undetermined_language() -> LanguageTag {
    "und".parse().expect("und is a well-formed language tag")
}
