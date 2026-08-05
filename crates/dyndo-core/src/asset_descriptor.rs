use language_tags::LanguageTag;
use opendal::Operator;
use relative_path::{RelativePath, RelativePathBuf};
use serde::{Deserialize, Serialize};

use crate::role::Role;
use crate::segment::SegmentOptions;
use crate::track::Track;

#[derive(Debug, thiserror::Error)]
pub enum AssetDescriptorError {
    #[error(transparent)]
    Storage(#[from] opendal::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AssetDescriptor {
    #[serde(skip)]
    path: RelativePathBuf,
    /// How the asset asks to be segmented, for requests that do not say.
    #[serde(default, skip_serializing_if = "is_default")]
    pub segment_options: SegmentOptions,
    pub tracks: Vec<TrackDescriptor>,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

impl AssetDescriptor {
    pub async fn read(op: &Operator, path: &str) -> Result<Self, AssetDescriptorError> {
        let bytes = op.read(path).await?;
        let mut descriptor: Self = serde_json::from_slice(&bytes.to_bytes())?;
        descriptor.path = RelativePathBuf::from(path);
        Ok(descriptor)
    }

    pub async fn read_or_new(
        op: &Operator,
        path: &RelativePath,
    ) -> Result<Self, AssetDescriptorError> {
        match Self::read(op, path.as_str()).await {
            Ok(descriptor) => Ok(descriptor),
            Err(AssetDescriptorError::Storage(error))
                if error.kind() == opendal::ErrorKind::NotFound =>
            {
                Ok(Self {
                    path: path.to_owned(),
                    ..Self::default()
                })
            }
            Err(error) => Err(error),
        }
    }

    pub fn track_path(&self, track: &TrackDescriptor) -> RelativePathBuf {
        self.path
            .parent()
            .unwrap_or(RelativePath::new(""))
            .join(&track.path)
    }

    pub fn track(&self, id: &str) -> Option<&TrackDescriptor> {
        self.tracks.iter().find(|track| track.id == id)
    }

    pub fn find_track_mut(&mut self, path: &RelativePath) -> Option<&mut TrackDescriptor> {
        let base = self
            .path
            .parent()
            .unwrap_or(RelativePath::new(""))
            .to_owned();
        self.tracks
            .iter_mut()
            .find(|track| base.join(&track.path) == path)
    }

    pub fn add_track(&mut self, track: &Track) -> &mut TrackDescriptor {
        let base = self.path.parent().unwrap_or(RelativePath::new(""));
        let path = base.relative(track.path());
        let index = self.tracks.len();
        self.tracks.push(TrackDescriptor::from_track(track, path));
        &mut self.tracks[index]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackDescriptor {
    pub id: String,
    /// Path relative to the asset descriptor; resolve it through
    /// [`AssetDescriptor::track_path`].
    path: RelativePathBuf,
    pub codec: String,
    #[serde(flatten)]
    pub kind: TrackKind,
}

impl TrackDescriptor {
    fn from_track(track: &Track, path: RelativePathBuf) -> Self {
        Self {
            id: track.id(),
            path,
            codec: track.codec().to_string(),
            kind: track.kind().clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TrackKind {
    Video(VideoKind),
    Audio(AudioKind),
    Text(TextKind),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoKind {
    pub width: u32,
    pub height: u32,
    pub frame_rate: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioKind {
    pub sample_rate: u32,
    pub channels: u16,
    #[serde(default = "undetermined_language")]
    pub language: LanguageTag,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextKind {
    #[serde(default = "undetermined_language")]
    pub language: LanguageTag,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
}

pub(crate) fn undetermined_language() -> LanguageTag {
    "und".parse().expect("und is a well-formed language tag")
}

#[cfg(test)]
mod tests {
    use opendal::services::Memory;

    use super::*;
    use crate::track::{Fragment, test_track};

    #[test]
    fn track_returns_descriptor_matching_id() {
        let asset = asset("assets/movie/asset.json", "video");

        assert_eq!(
            asset.track("video").map(|track| track.id.as_str()),
            Some("video")
        );
    }

    #[test]
    fn track_returns_none_for_unknown_id() {
        let asset = asset("asset.json", "video");

        assert!(asset.track("missing").is_none());
    }

    #[test]
    fn track_path_is_relative_to_nested_descriptor() {
        let asset = asset("assets/movie/asset.json", "video");

        assert_eq!(
            asset.track_path(&asset.tracks[0]),
            RelativePath::new("assets/movie/video.mp4")
        );
    }

    #[test]
    fn find_track_mut_matches_resolved_path() {
        let mut asset = asset("assets/movie/asset.json", "video");

        assert!(
            asset
                .find_track_mut(RelativePath::new("assets/movie/video.mp4"))
                .is_some()
        );
    }

    #[test]
    fn add_track_stores_path_relative_to_descriptor() {
        let mut asset = AssetDescriptor {
            path: RelativePathBuf::from("assets/movie/asset.json"),
            ..AssetDescriptor::default()
        };
        let track = test_track(
            video_kind(),
            1_000,
            vec![Fragment::new(0, 10, 1_000).unwrap()],
        );

        let descriptor = asset.add_track(&track);

        assert_eq!(descriptor.path, RelativePath::new("../../track.mp4"));
    }

    #[test]
    fn serialization_omits_default_segment_options() {
        let asset = asset("asset.json", "video");
        let json = serde_json::to_value(asset).unwrap();

        assert!(json.get("segment_options").is_none());
    }

    #[test]
    fn serialization_keeps_segment_options_the_asset_asked_for() {
        let mut asset = asset("asset.json", "video");
        asset.segment_options.min_length_ms = 3_000;
        let json = serde_json::to_value(asset).unwrap();

        assert_eq!(
            json.get("segment_options").and_then(|options| options
                .get("min_length")
                .and_then(serde_json::Value::as_u64)),
            Some(3_000)
        );
    }

    #[test]
    fn deserialization_reads_segment_options_and_defaults_the_rest() {
        let json = r#"{"segment_options":{"boundaries":[7400]},"tracks":[]}"#;

        let asset: AssetDescriptor = serde_json::from_str(json).unwrap();

        assert_eq!(asset.segment_options.boundaries, [7_400]);
        assert_eq!(asset.segment_options.min_length_ms, 0);
    }

    #[test]
    fn deserialization_accepts_the_short_and_long_option_aliases() {
        let json = r#"{"segment_options":{"sml":3000,"segment_boundaries":[7400]},"tracks":[]}"#;

        let asset: AssetDescriptor = serde_json::from_str(json).unwrap();

        assert_eq!(asset.segment_options.min_length_ms, 3_000);
        assert_eq!(asset.segment_options.boundaries, [7_400]);
    }

    #[test]
    fn json_round_trip_preserves_track_metadata() {
        let asset = asset("asset.json", "video");
        let json = serde_json::to_vec(&asset).unwrap();
        let decoded: AssetDescriptor = serde_json::from_slice(&json).unwrap();

        assert_eq!(decoded.tracks, asset.tracks);
    }

    #[test]
    fn language_tag_deserialization_accepts_bcp47_tag() {
        let kind: TextKind = serde_json::from_str(r#"{"language":"pt-BR"}"#).unwrap();

        assert_eq!(kind.language.as_str(), "pt-BR");
    }

    #[test]
    fn audio_language_defaults_to_und_when_missing() {
        let kind: AudioKind =
            serde_json::from_str(r#"{"sample_rate":48000,"channels":2}"#).unwrap();

        assert_eq!(kind.language.as_str(), "und");
    }

    #[test]
    fn text_language_defaults_to_und_when_missing() {
        let kind: TextKind = serde_json::from_str("{}").unwrap();

        assert_eq!(kind.language.as_str(), "und");
    }

    #[test]
    fn language_tag_deserialization_rejects_malformed_tag() {
        let result = serde_json::from_str::<TextKind>(r#"{"language":"not_a_tag"}"#);

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn read_or_new_creates_empty_descriptor_when_missing() {
        let op = Operator::new(Memory::default()).unwrap();

        let asset = AssetDescriptor::read_or_new(&op, RelativePath::new("nested/asset.json"))
            .await
            .unwrap();

        assert_eq!(asset.path, RelativePath::new("nested/asset.json"));
    }

    #[tokio::test]
    async fn read_or_new_preserves_malformed_json_error() {
        let op = Operator::new(Memory::default()).unwrap();
        op.write("asset.json", "not json").await.unwrap();

        let error = AssetDescriptor::read_or_new(&op, RelativePath::new("asset.json"))
            .await
            .unwrap_err();

        assert!(matches!(error, AssetDescriptorError::Json(_)));
    }

    fn asset(path: &str, id: &str) -> AssetDescriptor {
        AssetDescriptor {
            path: RelativePathBuf::from(path),
            tracks: vec![TrackDescriptor {
                id: id.to_string(),
                path: RelativePathBuf::from("video.mp4"),
                codec: "avc1.640028".to_string(),
                kind: video_kind(),
            }],
            ..AssetDescriptor::default()
        }
    }

    fn video_kind() -> TrackKind {
        TrackKind::Video(VideoKind {
            width: 1920,
            height: 1080,
            frame_rate: "25/1".to_string(),
        })
    }
}
