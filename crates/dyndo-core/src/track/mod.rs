mod discovered_cmaf_track;

pub use discovered_cmaf_track::{DiscoverError, DiscoveredCmafTrack};
use language_tags::LanguageTag;
use relative_path::RelativePathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{codec_config::CodecConfig, frame_rate::FrameRate, role::Role};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct VideoMetadata {
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct AudioMetadata {
    pub sample_rate: u32,
    pub channels: u16,
    #[serde(default = "language_und")]
    #[schemars(with = "String")]
    pub language: LanguageTag,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct TextMetadata {
    #[serde(default = "language_und")]
    #[schemars(with = "String")]
    pub language: LanguageTag,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct ImageMetadata {
    pub tile_size: u32,
    pub width: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct ThumbnailTrack {
    pub id: String,
    #[serde(flatten)]
    pub metadata: ImageMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct CmafTrack<M> {
    #[schemars(with = "String")]
    pub path: RelativePathBuf,
    pub codec: CodecConfig,
    pub bitrate: u64,
    #[serde(flatten)]
    pub metadata: M,
}

pub type CmafVideoTrack = CmafTrack<VideoMetadata>;
pub type CmafAudioTrack = CmafTrack<AudioMetadata>;
pub type CmafTextTrack = CmafTrack<TextMetadata>;

impl CmafTrack<VideoMetadata> {
    pub fn id(&self) -> String {
        format!(
            "{}_{}_{}",
            self.codec.family(),
            self.metadata.height,
            self.bitrate
        )
    }
}

impl CmafTrack<AudioMetadata> {
    pub fn id(&self) -> String {
        let metadata = &self.metadata;

        match metadata.role {
            Some(role) => format!(
                "{}_{}_{}_{}_{}_{}",
                self.codec.family(),
                metadata.sample_rate,
                metadata.channels,
                metadata.language,
                role,
                self.bitrate
            ),
            None => format!(
                "{}_{}_{}_{}_{}",
                self.codec.family(),
                metadata.sample_rate,
                metadata.channels,
                metadata.language,
                self.bitrate
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct SidecarTextTrack {
    #[schemars(with = "String")]
    pub path: RelativePathBuf,
    #[serde(flatten)]
    pub metadata: TextMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum TextTrack {
    Cmaf(CmafTextTrack),
    Sidecar(SidecarTextTrack),
}

impl TextTrack {
    pub fn id(&self) -> String {
        let metadata = match self {
            Self::Cmaf(track) => &track.metadata,
            Self::Sidecar(track) => &track.metadata,
        };

        match metadata.role {
            Some(role) => format!("text_{}_{}", metadata.language, role),
            None => format!("text_{}", metadata.language),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Track {
    #[serde(rename = "video")]
    Video(CmafVideoTrack),
    #[serde(rename = "audio")]
    Audio(CmafAudioTrack),
    #[serde(rename = "text")]
    Text(TextTrack),
    #[serde(rename = "thumbnail")]
    Thumbnail(ThumbnailTrack),
}

impl Track {
    pub fn id(&self) -> String {
        match self {
            Self::Video(track) => track.id(),
            Self::Audio(track) => track.id(),
            Self::Text(track) => track.id(),
            Self::Thumbnail(track) => track.id.clone(),
        }
    }
}

fn language_und() -> LanguageTag {
    "und".parse().expect("und is a well-formed language tag")
}
