mod discover;

use language_tags::LanguageTag;
use relative_path::RelativePathBuf;
use serde::{Deserialize, Serialize};
pub use discover::DiscoverError;

use crate::{codec_config::CodecConfig, frame_rate::FrameRate, role::Role};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct VideoMetadata {
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct AudioMetadata {
    pub sample_rate: u32,
    pub channels: u16,
    #[serde(default = "language_und")]
    pub language: LanguageTag,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct TextMetadata {
    #[serde(default = "language_und")]
    pub language: LanguageTag,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ImageMetadata {
    pub tile_size: u32,
    pub width: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct CmafTrack<M> {
    pub path: RelativePathBuf,
    pub codec: CodecConfig,
    #[serde(flatten)]
    pub metadata: M,
}

pub type CmafVideoTrack = CmafTrack<VideoMetadata>;
pub type CmafAudioTrack = CmafTrack<AudioMetadata>;
pub type CmafTextTrack = CmafTrack<TextMetadata>;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RawTrack<M> {
    pub path: RelativePathBuf,
    #[serde(flatten)]
    pub metadata: M,
}

pub type WebVttTextTrack = RawTrack<TextMetadata>;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum Track {
    #[serde(rename = "video")]
    CmafVideo(CmafVideoTrack),
    #[serde(rename = "audio")]
    CmafAudio(CmafAudioTrack),
    #[serde(rename = "text")]
    CmafText(CmafTextTrack),
    #[serde(rename = "webvtt")]
    WebVttText(WebVttTextTrack),
    #[serde(rename = "thumbnail")]
    Thumbnail(ImageMetadata),
}

fn language_und() -> LanguageTag {
    "und".parse().expect("und is a well-formed language tag")
}
