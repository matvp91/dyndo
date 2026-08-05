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
    #[serde(default, skip_serializing_if = "is_zero")]
    pub min_segment_length: u64,
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

fn is_zero(value: &u64) -> bool {
    *value == 0
}
