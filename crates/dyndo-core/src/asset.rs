use std::time::Duration;

use relative_path::{RelativePath, RelativePathBuf};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::{DurationSecondsWithFrac, serde_as};
use thiserror::Error;

use crate::{
    storage::{Storage, StorageError},
    track::Track,
};

pub const ASSET_SCHEMA_URL: &str = concat!(
    "https://matvp91.github.io/dyndo/",
    env!("CARGO_PKG_VERSION"),
    "/schema.json"
);

#[serde_as]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    #[serde(rename = "$schema", default = "asset_schema_url")]
    schema: String,
    #[serde(skip)]
    #[schemars(skip)]
    path: RelativePathBuf,
    /// Splice points, in seconds from the presentation start.
    #[serde_as(as = "Vec<DurationSecondsWithFrac<f64>>")]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boundaries: Vec<Duration>,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Error)]
pub enum AssetError {
    #[error("failed to access storage: {0}")]
    Storage(#[from] StorageError),
    #[error("failed to read asset: {0}")]
    Read(#[from] opendal::Error),
    #[error("failed to deserialize asset: {0}")]
    Deserialize(serde_json::Error),
    #[error("failed to serialize asset: {0}")]
    Serialize(serde_json::Error),
}

impl Asset {
    pub fn new(path: RelativePathBuf) -> Self {
        Self {
            schema: asset_schema_url(),
            path,
            boundaries: Vec::new(),
            tracks: Vec::new(),
        }
    }

    pub async fn read(path: &RelativePath) -> Result<Self, AssetError> {
        let bytes = Storage::source_op()?.read(path.as_str()).await?;
        let mut asset: Self =
            serde_json::from_slice(&bytes.to_bytes()).map_err(AssetError::Deserialize)?;
        asset.path = path.to_owned();

        Ok(asset)
    }

    pub async fn write(&self) -> Result<(), AssetError> {
        let asset = serde_json::to_vec_pretty(self).map_err(AssetError::Serialize)?;
        Storage::source_op()?
            .write(self.path.as_str(), asset)
            .await?;

        Ok(())
    }

    pub fn track_path(&self, track: &Track) -> Option<RelativePathBuf> {
        let path = match track {
            Track::Video(track) => track.path.as_relative_path(),
            Track::Audio(track) => track.path.as_relative_path(),
            Track::Text(text_track) => match text_track {
                crate::track::TextTrack::Cmaf(track) => track.path.as_relative_path(),
                crate::track::TextTrack::Sidecar(track) => track.path.as_relative_path(),
            },
            Track::Thumbnail(_) => return None,
        };
        let base = self.path.parent().unwrap_or(RelativePath::new(""));

        Some(base.join(path))
    }
}

fn asset_schema_url() -> String {
    ASSET_SCHEMA_URL.to_owned()
}
