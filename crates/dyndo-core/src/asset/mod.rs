use opendal::Operator;
use relative_path::{RelativePath, RelativePathBuf};
use serde::{Deserialize, Serialize};

use self::descriptor::ThumbnailTrackDescriptor;
use self::track::{SourceTrackDescriptor, SyntheticTrackDescriptor, TrackDescriptor};
use crate::segment_options::SegmentOptions;
use crate::track::SourceTrack;

pub mod descriptor;
pub mod kind;
pub mod track;

#[derive(Debug, thiserror::Error)]
pub enum AssetDescriptorError {
    #[error(transparent)]
    Storage(#[from] opendal::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

    pub fn track_path(&self, track: &SourceTrackDescriptor) -> RelativePathBuf {
        self.path
            .parent()
            .unwrap_or(RelativePath::new(""))
            .join(track.source_path())
    }

    pub fn find_source_track_by_id(&self, id: &str) -> Option<&SourceTrackDescriptor> {
        self.source_tracks().find(|track| track.id() == id)
    }

    pub fn find_thumbnail_track_by_id(&self, id: &str) -> Option<&ThumbnailTrackDescriptor> {
        self.thumbnail_tracks().find(|track| track.id == id)
    }

    pub fn source_tracks(&self) -> impl Iterator<Item = &SourceTrackDescriptor> {
        self.tracks.iter().filter_map(TrackDescriptor::source)
    }

    pub fn synthetic_tracks(&self) -> impl Iterator<Item = &SyntheticTrackDescriptor> {
        self.tracks.iter().filter_map(TrackDescriptor::synthetic)
    }

    pub fn thumbnail_tracks(&self) -> impl Iterator<Item = &ThumbnailTrackDescriptor> {
        self.synthetic_tracks()
            .map(SyntheticTrackDescriptor::thumbnail)
    }

    pub fn find_track_by_path(
        &mut self,
        path: &RelativePath,
    ) -> Option<&mut SourceTrackDescriptor> {
        let base = self
            .path
            .parent()
            .unwrap_or(RelativePath::new(""))
            .to_owned();
        self.tracks
            .iter_mut()
            .filter_map(TrackDescriptor::source_mut)
            .find(|track| base.join(track.source_path()) == path)
    }

    pub fn add_source_track(&mut self, track: &SourceTrack) -> &mut SourceTrackDescriptor {
        let base = self.path.parent().unwrap_or(RelativePath::new(""));
        let path = base.relative(track.source_path());
        let index = self.tracks.len();
        self.tracks.push(TrackDescriptor::Source(
            SourceTrackDescriptor::from_source_track(track, path),
        ));
        match &mut self.tracks[index] {
            TrackDescriptor::Source(track) => track,
            TrackDescriptor::Synthetic(_) => unreachable!("a source track was just inserted"),
        }
    }
}
