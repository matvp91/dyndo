use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Main,
    Alternate,
    Commentary,
    Dub,
    Description,
    EnhancedAudioIntelligibility,
    Subtitle,
    Caption,
    ForcedSubtitle,
}

impl Role {
    pub fn as_str(self) -> &'static str {
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
