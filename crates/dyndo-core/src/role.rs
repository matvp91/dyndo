//! Track roles: the author-declared *purpose* of an audio or text track,
//! serialized as the DASH `Role@value` and mapped into HLS selection and
//! characteristics attributes by the manifest builders.

use serde::{Deserialize, Serialize};

/// The purpose of an audio track (DASH role scheme `urn:mpeg:dash:role:2011`).
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

/// The purpose of a timed-text track (DASH role scheme `urn:mpeg:dash:role:2011`).
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

impl AudioRole {
    /// The kebab-case DASH `Role@value` string for this role.
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

    /// Parse a DASH `Role@value` string into an [`AudioRole`], or `None` if it
    /// is not one of this type's values. The inverse of [`AudioRole::as_str`].
    pub fn from_value(s: &str) -> Option<AudioRole> {
        match s {
            "main" => Some(AudioRole::Main),
            "alternate" => Some(AudioRole::Alternate),
            "commentary" => Some(AudioRole::Commentary),
            "dub" => Some(AudioRole::Dub),
            "description" => Some(AudioRole::Description),
            "enhanced-audio-intelligibility" => Some(AudioRole::EnhancedAudioIntelligibility),
            _ => None,
        }
    }
}

impl TextRole {
    /// The kebab-case DASH `Role@value` string for this role.
    pub fn as_str(self) -> &'static str {
        match self {
            TextRole::Subtitle => "subtitle",
            TextRole::Caption => "caption",
            TextRole::ForcedSubtitle => "forced-subtitle",
        }
    }

    /// Parse a DASH `Role@value` string into a [`TextRole`], or `None` if it is
    /// not one of this type's values. The inverse of [`TextRole::as_str`].
    pub fn from_value(s: &str) -> Option<TextRole> {
        match s {
            "subtitle" => Some(TextRole::Subtitle),
            "caption" => Some(TextRole::Caption),
            "forced-subtitle" => Some(TextRole::ForcedSubtitle),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_roles_serialize_kebab_case() {
        assert_eq!(
            serde_json::to_string(&TextRole::Subtitle).unwrap(),
            "\"subtitle\""
        );
        assert_eq!(
            serde_json::to_string(&TextRole::Caption).unwrap(),
            "\"caption\""
        );
        assert_eq!(
            serde_json::to_string(&TextRole::ForcedSubtitle).unwrap(),
            "\"forced-subtitle\""
        );
    }

    #[test]
    fn audio_roles_serialize_kebab_case() {
        assert_eq!(serde_json::to_string(&AudioRole::Main).unwrap(), "\"main\"");
        assert_eq!(
            serde_json::to_string(&AudioRole::EnhancedAudioIntelligibility).unwrap(),
            "\"enhanced-audio-intelligibility\""
        );
    }

    #[test]
    fn unknown_value_is_rejected() {
        assert!(serde_json::from_str::<AudioRole>("\"karaoke\"").is_err());
        assert!(serde_json::from_str::<TextRole>("\"caption\"").is_ok());
        assert!(serde_json::from_str::<AudioRole>("\"caption\"").is_err());
    }

    #[test]
    fn as_str_matches_serde_for_every_variant() {
        for r in [
            AudioRole::Main,
            AudioRole::Alternate,
            AudioRole::Commentary,
            AudioRole::Dub,
            AudioRole::Description,
            AudioRole::EnhancedAudioIntelligibility,
        ] {
            assert_eq!(
                serde_json::to_string(&r).unwrap(),
                format!("\"{}\"", r.as_str())
            );
        }
        for r in [
            TextRole::Subtitle,
            TextRole::Caption,
            TextRole::ForcedSubtitle,
        ] {
            assert_eq!(
                serde_json::to_string(&r).unwrap(),
                format!("\"{}\"", r.as_str())
            );
        }
    }

    #[test]
    fn from_value_is_the_inverse_of_as_str() {
        for r in [
            AudioRole::Main,
            AudioRole::Alternate,
            AudioRole::Commentary,
            AudioRole::Dub,
            AudioRole::Description,
            AudioRole::EnhancedAudioIntelligibility,
        ] {
            assert_eq!(AudioRole::from_value(r.as_str()), Some(r));
        }
        for r in [
            TextRole::Subtitle,
            TextRole::Caption,
            TextRole::ForcedSubtitle,
        ] {
            assert_eq!(TextRole::from_value(r.as_str()), Some(r));
        }
    }

    #[test]
    fn from_value_rejects_unknown_and_cross_type_values() {
        assert_eq!(AudioRole::from_value("karaoke"), None);
        assert_eq!(AudioRole::from_value(""), None);
        // "subtitle" is a text role, not an audio role, and vice versa.
        assert_eq!(AudioRole::from_value("subtitle"), None);
        assert_eq!(TextRole::from_value("main"), None);
    }
}
