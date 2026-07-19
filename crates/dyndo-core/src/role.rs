//! Track roles: the author-declared *purpose* of an audio or text track,
//! serialized as kebab-case strings (the DASH `Role@value` vocabulary,
//! scheme `urn:mpeg:dash:role:2011`).

use serde::{Deserialize, Serialize};

/// The purpose of an audio track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AudioRole {
    /// The primary audio.
    Main,
    /// An alternate version of the main audio.
    Alternate,
    /// Commentary (e.g. director's commentary).
    Commentary,
    /// A dubbed rendition in another language.
    Dub,
    /// Audio description for viewers who are blind or have low vision.
    Description,
    /// Dialogue enhanced for intelligibility.
    EnhancedAudioIntelligibility,
}

impl AudioRole {
    /// The role's kebab-case wire name (e.g. `"main"`,
    /// `"enhanced-audio-intelligibility"`): the DASH `Role@value` string,
    /// also used to qualify HLS rendition names.
    pub fn as_str(self) -> &'static str {
        match self {
            AudioRole::Main => "main",
            AudioRole::Alternate => "alternate",
            AudioRole::Commentary => "commentary",
            AudioRole::Dub => "dub",
            AudioRole::Description => "description",
            AudioRole::EnhancedAudioIntelligibility => "enhanced-audio-intelligibility",
        }
    }
}

/// The purpose of a timed-text track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextRole {
    /// Translation subtitles (dialogue only).
    Subtitle,
    /// SDH / closed captions (dialogue plus non-dialogue sound description).
    Caption,
    /// Forced narrative subtitles (foreign dialogue or on-screen text), shown
    /// even when subtitles are otherwise off.
    ForcedSubtitle,
}

impl TextRole {
    /// The role's kebab-case wire name (e.g. `"forced-subtitle"`): the DASH
    /// `Role@value` string.
    pub fn as_str(self) -> &'static str {
        match self {
            TextRole::Subtitle => "subtitle",
            TextRole::Caption => "caption",
            TextRole::ForcedSubtitle => "forced-subtitle",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_role_as_str_matches_the_wire_name() {
        for role in [
            AudioRole::Main,
            AudioRole::Alternate,
            AudioRole::Commentary,
            AudioRole::Dub,
            AudioRole::Description,
            AudioRole::EnhancedAudioIntelligibility,
        ] {
            assert_eq!(serde_json::to_value(role).unwrap(), role.as_str());
        }
    }

    #[test]
    fn text_role_as_str_matches_the_wire_name() {
        for role in [
            TextRole::Subtitle,
            TextRole::Caption,
            TextRole::ForcedSubtitle,
        ] {
            assert_eq!(serde_json::to_value(role).unwrap(), role.as_str());
        }
    }
}
