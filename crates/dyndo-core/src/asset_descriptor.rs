use opendal::Operator;
use relative_path::{RelativePath, RelativePathBuf};
use serde::{Deserialize, Serialize};

use crate::role::Role;
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segment_boundaries: Vec<u64>,
    pub tracks: Vec<TrackDescriptor>,
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
    pub language: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextKind {
    pub language: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
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
    fn serialization_omits_default_segment_boundaries() {
        let asset = asset("asset.json", "video");
        let json = serde_json::to_value(asset).unwrap();

        assert!(json.get("segment_boundaries").is_none());
    }

    #[test]
    fn json_round_trip_preserves_track_metadata() {
        let asset = asset("asset.json", "video");
        let json = serde_json::to_vec(&asset).unwrap();
        let decoded: AssetDescriptor = serde_json::from_slice(&json).unwrap();

        assert_eq!(decoded.tracks, asset.tracks);
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
