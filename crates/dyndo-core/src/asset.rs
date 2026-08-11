use opendal::Operator;
use relative_path::{RelativePath, RelativePathBuf};
use serde::{Deserialize, Serialize};

use crate::segment_options::SegmentOptions;
use crate::track::ResolvedSourceTrack;
use crate::track::{SourceTrack, ThumbnailTrack, Track};

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error(transparent)]
    Storage(#[from] opendal::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    #[serde(skip)]
    path: RelativePathBuf,
    /// How the asset asks to be segmented, for requests that do not say.
    #[serde(default, skip_serializing_if = "is_default")]
    pub segment_options: SegmentOptions,
    pub tracks: Vec<Track>,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

impl Asset {
    pub async fn read(op: &Operator, path: &str) -> Result<Self, AssetError> {
        let bytes = op.read(path).await?;
        let mut asset: Self = serde_json::from_slice(&bytes.to_bytes())?;
        asset.path = RelativePathBuf::from(path);
        Ok(asset)
    }

    pub async fn read_or_new(op: &Operator, path: &RelativePath) -> Result<Self, AssetError> {
        match Self::read(op, path.as_str()).await {
            Ok(asset) => Ok(asset),
            Err(AssetError::Storage(error)) if error.kind() == opendal::ErrorKind::NotFound => {
                Ok(Self {
                    path: path.to_owned(),
                    ..Self::default()
                })
            }
            Err(error) => Err(error),
        }
    }

    pub async fn write(&self, op: &Operator) -> Result<(), AssetError> {
        op.write(self.path.as_str(), serde_json::to_vec_pretty(self)?)
            .await?;
        Ok(())
    }

    pub fn track_path(&self, track: &SourceTrack) -> RelativePathBuf {
        self.path
            .parent()
            .unwrap_or(RelativePath::new(""))
            .join(track.source_path())
    }

    pub fn find_source_track_by_id(&self, id: &str) -> Option<&SourceTrack> {
        self.source_tracks().find(|track| track.id() == id)
    }

    pub fn find_thumbnail_track_by_id(&self, id: &str) -> Option<&ThumbnailTrack> {
        self.thumbnail_tracks().find(|track| track.id == id)
    }

    pub fn source_tracks(&self) -> impl Iterator<Item = &SourceTrack> {
        self.tracks.iter().filter_map(Track::source)
    }

    pub fn thumbnail_tracks(&self) -> impl Iterator<Item = &ThumbnailTrack> {
        self.tracks.iter().filter_map(Track::thumbnail)
    }

    pub fn find_track_by_path(&mut self, path: &RelativePath) -> Option<&mut SourceTrack> {
        let base = self
            .path
            .parent()
            .unwrap_or(RelativePath::new(""))
            .to_owned();
        self.tracks
            .iter_mut()
            .filter_map(Track::source_mut)
            .find(|track| base.join(track.source_path()) == path)
    }

    pub fn add_source_track(&mut self, track: &ResolvedSourceTrack) -> &mut SourceTrack {
        let base = self.path.parent().unwrap_or(RelativePath::new(""));
        let path = base.relative(track.source_path());
        let index = self.tracks.len();
        self.tracks
            .push(Track::Source(SourceTrack::from_resolved(track, path)));
        match &mut self.tracks[index] {
            Track::Source(track) => track,
            Track::Thumbnail(_) => unreachable!("a source track was just inserted"),
        }
    }
}
