use opendal::Operator;
use relative_path::{RelativePath, RelativePathBuf};
use serde::{Deserialize, Serialize};

use super::segment_options::SegmentOptions;
use super::thumbnail_descriptor::ThumbnailDescriptor;
use super::track::Track;
use super::track_descriptor::TrackDescriptor;

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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thumbnails: Vec<ThumbnailDescriptor>,
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

    pub fn find_track_by_id(&self, id: &str) -> Option<&TrackDescriptor> {
        self.tracks.iter().find(|track| track.id == id)
    }

    pub fn find_thumbnail_by_id(&self, id: &str) -> Option<&ThumbnailDescriptor> {
        self.thumbnails.iter().find(|thumbnail| thumbnail.id == id)
    }

    pub fn find_track_by_path(&mut self, path: &RelativePath) -> Option<&mut TrackDescriptor> {
        let index = self
            .tracks
            .iter()
            .position(|track| self.track_path(track) == path)?;

        self.tracks.get_mut(index)
    }

    pub fn add_track(&mut self, track: &Track) -> &mut TrackDescriptor {
        let base = self.path.parent().unwrap_or(RelativePath::new(""));
        let path = base.relative(track.path());
        let index = self.tracks.len();
        self.tracks.push(TrackDescriptor::from_track(track, path));
        &mut self.tracks[index]
    }
}
