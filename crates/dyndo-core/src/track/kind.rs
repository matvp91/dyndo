use language_tags::LanguageTag;
use serde::{Deserialize, Serialize};

use crate::role::Role;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CmafTrackKind {
    Video(VideoKind),
    Audio(AudioKind),
    Text(TextKind),
}

/// The document format carried by a timed-text source track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimedTextKind {
    WebVtt(TextKind),
}

impl TimedTextKind {
    /// Returns the text metadata shared by all timed-text formats.
    pub fn text(&self) -> &TextKind {
        match self {
            Self::WebVtt(kind) => kind,
        }
    }

    /// Returns whether this source is a WebVTT document.
    pub fn is_web_vtt(&self) -> bool {
        matches!(self, Self::WebVtt(_))
    }
}

/// The output type of a synthetic track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntheticTrackKind {
    Thumbnail(ThumbnailKind),
}

impl CmafTrackKind {
    /// Returns the DASH media content type represented by a track of this kind.
    pub fn content_type(&self) -> &'static str {
        match self {
            Self::Video(_) => "video",
            Self::Audio(_) => "audio",
            Self::Text(_) => "text",
        }
    }

    /// Returns the media type of the CMAF representation of a track of this kind.
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Video(_) => "video/mp4",
            Self::Audio(_) => "audio/mp4",
            Self::Text(_) => "application/mp4",
        }
    }
}

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
