//! Track roles serialized as kebab-case strings.

use serde::{Deserialize, Serialize};

/// The author-declared purpose of a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    /// The primary track.
    Main,
    /// An alternate version of the main track.
    Alternate,
    /// Commentary (e.g. director's commentary).
    Commentary,
    /// A dubbed rendition in another language.
    Dub,
    /// Audio description for viewers who are blind or have low vision.
    Description,
    /// Dialogue enhanced for intelligibility.
    EnhancedAudioIntelligibility,
    /// Translation subtitles (dialogue only).
    Subtitle,
    /// SDH / closed captions (dialogue plus non-dialogue sound description).
    Caption,
    /// Forced narrative subtitles (foreign dialogue or on-screen text), shown
    /// even when subtitles are otherwise off.
    ForcedSubtitle,
}

impl Role {
    /// Return the role's canonical kebab-case wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Alternate => "alternate",
            Self::Commentary => "commentary",
            Self::Dub => "dub",
            Self::Description => "description",
            Self::EnhancedAudioIntelligibility => "enhanced-audio-intelligibility",
            Self::Subtitle => "subtitle",
            Self::Caption => "caption",
            Self::ForcedSubtitle => "forced-subtitle",
        }
    }
}
