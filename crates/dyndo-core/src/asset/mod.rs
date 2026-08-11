use opendal::Operator;
use relative_path::{RelativePath, RelativePathBuf};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::segment_options::SegmentOptions;
use crate::track::thumbnail::ThumbnailTrack;
use crate::track::{ResolvedTrack, SourceTrack, Track};

mod resolve;

pub use resolve::{AssetResolveError, ResolvedAsset};

/// The versioned JSON Schema used by descriptors written by this build.
pub const ASSET_SCHEMA_URL: &str = concat!(
    "https://matvp91.github.io/dyndo/",
    env!("CARGO_PKG_VERSION"),
    "/schema.json"
);

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error(transparent)]
    Storage(#[from] opendal::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("resolved track is not backed by an asset source")]
    MissingSourcePath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    /// JSON Schema used to validate this descriptor.
    #[serde(rename = "$schema", default = "asset_schema_url")]
    schema: String,
    #[serde(skip)]
    #[schemars(skip)]
    path: RelativePathBuf,
    /// How the asset asks to be segmented, for requests that do not say.
    #[serde(default, skip_serializing_if = "is_default")]
    pub segment_options: SegmentOptions,
    pub tracks: Vec<Track>,
}

impl Default for Asset {
    fn default() -> Self {
        Self {
            schema: asset_schema_url(),
            path: RelativePathBuf::default(),
            segment_options: SegmentOptions::default(),
            tracks: Vec::new(),
        }
    }
}

fn asset_schema_url() -> String {
    ASSET_SCHEMA_URL.to_owned()
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

    pub fn find_source_track_by_path(&mut self, path: &RelativePath) -> Option<&mut SourceTrack> {
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

    /// Adds a resolved track backed by a stored asset source.
    ///
    /// Returns an error when the resolved representation has no source path.
    pub fn add_source_track(
        &mut self,
        track: &ResolvedTrack,
    ) -> Result<&mut SourceTrack, AssetError> {
        let base = self.path.parent().unwrap_or(RelativePath::new(""));
        let source_path = track.source_path().ok_or(AssetError::MissingSourcePath)?;
        let path = base.relative(source_path);
        let source =
            SourceTrack::from_resolved(track, path).ok_or(AssetError::MissingSourcePath)?;
        let index = self.tracks.len();
        self.tracks.push(Track::Source(source));
        Ok(match &mut self.tracks[index] {
            Track::Source(track) => track,
            Track::Thumbnail(_) => unreachable!("a source track was just inserted"),
        })
    }
}
