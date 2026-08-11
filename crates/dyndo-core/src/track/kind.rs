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

pub(crate) fn undetermined_language() -> LanguageTag {
    "und".parse().expect("und is a well-formed language tag")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CmafTrackKind {
    Video(VideoKind),
    Audio(AudioKind),
    Text(TextKind),
}

/// The document format carried by a timed-text source track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
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

    /// Returns the mutable text metadata shared by all timed-text formats.
    pub fn text_mut(&mut self) -> &mut TextKind {
        match self {
            Self::WebVtt(kind) => kind,
        }
    }

    /// Returns the serialized type for this timed-text format.
    pub fn asset_type(&self) -> &'static str {
        match self {
            Self::WebVtt(_) => "webvtt",
        }
    }

    /// Returns whether this source is a WebVTT document.
    pub fn is_web_vtt(&self) -> bool {
        matches!(self, Self::WebVtt(_))
    }
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

    pub fn video(&self) -> Option<&VideoKind> {
        match self {
            Self::Video(kind) => Some(kind),
            Self::Audio(_) | Self::Text(_) => None,
        }
    }

    /// Returns the audio metadata when this is an audio track.
    pub fn audio(&self) -> Option<&AudioKind> {
        match self {
            Self::Video(_) | Self::Text(_) => None,
            Self::Audio(kind) => Some(kind),
        }
    }

    pub fn language(&self) -> Option<&LanguageTag> {
        match self {
            Self::Video(_) => None,
            Self::Audio(kind) => Some(&kind.language),
            Self::Text(kind) => Some(&kind.language),
        }
    }

    pub fn role(&self) -> Option<Role> {
        match self {
            Self::Video(_) => None,
            Self::Audio(kind) => kind.role,
            Self::Text(kind) => kind.role,
        }
    }

    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text(_))
    }

    /// Returns mutable language and role metadata when the track is not video.
    pub fn language_and_role_mut(&mut self) -> Option<(&mut LanguageTag, &mut Option<Role>)> {
        match self {
            Self::Video(_) => None,
            Self::Audio(kind) => Some((&mut kind.language, &mut kind.role)),
            Self::Text(kind) => Some((&mut kind.language, &mut kind.role)),
        }
    }
}
