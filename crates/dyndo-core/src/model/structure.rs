//! The `asset.json` serde contract.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Track {
    Video(VideoTrack),
    Audio(AudioTrack),
}

impl Track {
    pub fn id(&self) -> &str {
        match self {
            Track::Video(t) => &t.id,
            Track::Audio(t) => &t.id,
        }
    }

    pub fn source(&self) -> &str {
        match self {
            Track::Video(t) => &t.source,
            Track::Audio(t) => &t.source,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoTrack {
    pub id: String,
    pub source: String,
    pub fourcc: String,
    pub timescale: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioTrack {
    pub id: String,
    pub source: String,
    pub fourcc: String,
    pub timescale: u32,
    pub sample_rate: u32,
    pub channels: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_track_serialises_with_type_tag_and_snake_case() {
        let asset = Asset {
            tracks: vec![Track::Video(VideoTrack {
                id: "video_avc_1080_4807".into(),
                source: "video_avc_1080.mp4".into(),
                fourcc: "avc1".into(),
                timescale: 90000,
                width: 1920,
                height: 1080,
            })],
        };
        let json = serde_json::to_value(&asset).unwrap();
        let t = &json["tracks"][0];
        assert_eq!(t["type"], "video");
        assert_eq!(t["fourcc"], "avc1");
        assert_eq!(t["width"], 1920);
    }

    #[test]
    fn audio_language_absent_is_omitted_and_round_trips() {
        let track = Track::Audio(AudioTrack {
            id: "audio_aac_und_2_197".into(),
            source: "a.mp4".into(),
            fourcc: "mp4a".into(),
            timescale: 48000,
            sample_rate: 48000,
            channels: 2,
            language: None,
        });
        let json = serde_json::to_value(&track).unwrap();
        assert!(json.get("language").is_none());
        let back: Track = serde_json::from_value(json).unwrap();
        assert_eq!(back, track);
        assert_eq!(back.id(), "audio_aac_und_2_197");
    }
}
