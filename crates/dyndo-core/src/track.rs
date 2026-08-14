use language_tags::LanguageTag;
use relative_path::RelativePathBuf;
use serde::{Deserialize, Serialize};

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

pub type WebVttTrack = RawTrack<TextMetadata>;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum TextTrack {
    Cmaf(CmafTextTrack),
    WebVtt(WebVttTrack),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Track {
    Video(CmafVideoTrack),
    Audio(CmafAudioTrack),
    Text(TextTrack),
    Thumbnail(ImageMetadata),
}

fn language_und() -> LanguageTag {
    "und".parse().expect("und is a well-formed language tag")
}

#[cfg(test)]
mod tests {
    use relative_path::RelativePathBuf;

    use super::{CmafTrack, RawTrack, TextMetadata, TextTrack, Track, VideoMetadata};
    use crate::frame_rate::FrameRate;

    #[test]
    fn track_should_serialize_with_a_type_and_flattened_payload() {
        let track = Track::Video(CmafTrack {
            path: RelativePathBuf::from("media/video.mp4"),
            codec: "avc1.64001f".parse().expect("codec string should be valid"),
            metadata: VideoMetadata {
                width: 1920,
                height: 1080,
                frame_rate: FrameRate::new(25, 1).expect("frame rate should be valid"),
            },
        });

        let value = serde_json::to_value(track).expect("track should serialize");

        assert_eq!(
            value,
            serde_json::json!({
                "type": "video",
                "path": "media/video.mp4",
                "codec": "avc1.64001f",
                "width": 1920,
                "height": 1080,
                "frame_rate": "25/1",
            })
        );
    }

    #[test]
    fn web_vtt_track_should_serialize_without_a_format() {
        let track = Track::Text(TextTrack::WebVtt(RawTrack {
            path: RelativePathBuf::from("subtitles/en.vtt"),
            metadata: TextMetadata {
                language: "en".parse().expect("en is a well-formed language tag"),
                role: None,
            },
        }));

        let value = serde_json::to_value(track).expect("track should serialize");

        assert_eq!(
            value,
            serde_json::json!({
                "type": "text",
                "path": "subtitles/en.vtt",
                "language": "en",
            })
        );
    }

    #[test]
    fn text_track_with_a_codec_should_deserialize_as_cmaf() {
        let track = serde_json::from_value(serde_json::json!({
            "type": "text",
            "path": "subtitles/en.mp4",
            "codec": "mp4a.40.2",
            "language": "en",
        }))
        .expect("track should deserialize");

        assert_eq!(
            track,
            Track::Text(TextTrack::Cmaf(CmafTrack {
                path: RelativePathBuf::from("subtitles/en.mp4"),
                codec: "mp4a.40.2".parse().expect("codec string should be valid"),
                metadata: TextMetadata {
                    language: "en".parse().expect("en is a well-formed language tag"),
                    role: None,
                },
            }))
        );
    }
}
