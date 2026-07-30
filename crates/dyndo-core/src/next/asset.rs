//! The asset descriptor and its storage I/O.

use bytes::Buf;
use opendal::{ErrorKind, Operator};
use relative_path::{RelativePath, RelativePathBuf};
use serde::{Deserialize, Serialize};

use super::track::Track;
use crate::error::CoreError;

/// An asset descriptor and its tracks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    /// The resolved storage path of this asset descriptor.
    #[serde(skip)]
    pub path: RelativePathBuf,
    /// Segment boundaries, in milliseconds from the start of the presentation.
    #[serde(
        rename = "segment_boundaries",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub segment_boundaries_ms: Vec<u64>,
    /// The asset's tracks, in descriptor order.
    pub tracks: Vec<Track>,
}

impl Asset {
    /// Read an asset descriptor and resolve its track paths. If the descriptor
    /// does not exist, return an empty asset rooted at `path`.
    ///
    /// # Errors
    /// Returns [`CoreError::Storage`] when the descriptor cannot be read for a
    /// reason other than it not existing, or [`CoreError::Descriptor`] when it
    /// is not valid descriptor JSON.
    pub async fn read(op: &Operator, path: &str) -> Result<Asset, CoreError> {
        let descriptor_path = RelativePathBuf::from(path);
        let buf = match op.read(path).await {
            Ok(buf) => buf,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(Asset {
                    path: descriptor_path,
                    segment_boundaries_ms: Vec::new(),
                    tracks: Vec::new(),
                });
            }
            Err(error) => return Err(error.into()),
        };

        let mut asset: Asset = serde_json::from_reader(buf.reader())?;
        asset.path = descriptor_path;

        let directory = asset_directory(&asset.path);
        for track in &mut asset.tracks {
            track.path = directory.join(&track.path).normalize();
        }

        Ok(asset)
    }

    /// Add a track to this asset.
    ///
    /// # Errors
    /// Track discovery and its errors are not implemented yet.
    pub async fn add_track(
        &mut self,
        _op: &Operator,
        _path: &str,
    ) -> Result<&mut Track, CoreError> {
        unimplemented!("track discovery is not implemented yet")
    }

    /// Serialize this asset with descriptor-relative track paths and write it.
    ///
    /// # Errors
    /// Returns [`CoreError::Descriptor`] when serialization fails, or
    /// [`CoreError::Storage`] when the descriptor cannot be written.
    pub async fn write(&self, op: &Operator) -> Result<(), CoreError> {
        let mut wire_asset = self.clone();
        let directory = asset_directory(&self.path);

        for track in &mut wire_asset.tracks {
            track.path = directory.relative(&track.path);
        }

        let bytes = serde_json::to_vec_pretty(&wire_asset)?;
        op.write(self.path.as_str(), bytes).await?;
        Ok(())
    }
}

fn asset_directory(path: &RelativePath) -> &RelativePath {
    path.parent().unwrap_or_else(|| RelativePath::new(""))
}
