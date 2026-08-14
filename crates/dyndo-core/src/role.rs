use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Display, EnumString)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
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
