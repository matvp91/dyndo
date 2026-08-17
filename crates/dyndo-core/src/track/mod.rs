mod discovered_cmaf_track;
mod track_discover;

pub use track_discover::{DiscoverError, TrackDiscover};
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
pub struct CmafTrack<M> {
    #[schemars(with = "String")]
    pub path: RelativePathBuf,
    pub codec: CodecConfig,
    #[serde(flatten)]
    pub metadata: M,
}

pub type CmafVideoTrack = CmafTrack<VideoMetadata>;
pub type CmafAudioTrack = CmafTrack<AudioMetadata>;
pub type CmafTextTrack = CmafTrack<TextMetadata>;

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
    Thumbnail(ImageMetadata),
}

fn language_und() -> LanguageTag {
    "und".parse().expect("und is a well-formed language tag")
}
