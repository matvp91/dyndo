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
