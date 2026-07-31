//! The asset descriptor and its storage I/O.

use bytes::Buf;
use opendal::{ErrorKind, Operator};
use relative_path::{RelativePath, RelativePathBuf};
use serde::{Deserialize, Serialize};

use super::error::Error;
use super::track::Track;

/// An asset descriptor and its tracks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    /// The resolved storage path of this asset descriptor.
    #[serde(skip)]
    pub path: RelativePathBuf,
    /// Minimum segment length in milliseconds. Zero keeps source fragments.
    #[serde(
        rename = "min_segment_length",
        default,
        skip_serializing_if = "is_zero"
    )]
    pub min_segment_length_ms: u64,
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
    /// Returns an error when the descriptor cannot be read or decoded.
    pub async fn read(op: &Operator, path: &str) -> Result<Asset, Error> {
        let descriptor_path = RelativePathBuf::from(path);
        let buf = match op.read(path).await {
            Ok(buf) => buf,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(Asset {
                    path: descriptor_path,
                    min_segment_length_ms: 0,
                    segment_boundaries_ms: Vec::new(),
                    tracks: Vec::new(),
                });
            }
            Err(source) => {
                return Err(Error::ReadDescriptor {
                    path: descriptor_path,
                    source,
                });
            }
        };

        let mut asset: Asset =
            serde_json::from_reader(buf.reader()).map_err(|source| Error::DecodeDescriptor {
                path: descriptor_path.clone(),
                source,
            })?;
        asset.path = descriptor_path;

        let directory = asset_directory(&asset.path);
        for track in &mut asset.tracks {
            track.path = directory.join(&track.path).normalize();
        }

        Ok(asset)
    }

    /// Serialize this asset with descriptor-relative track paths and write it.
    ///
    /// # Errors
    /// Returns an error when the descriptor cannot be encoded or written.
    pub async fn write(&self, op: &Operator) -> Result<(), Error> {
        let mut wire_asset = self.clone();
        let directory = asset_directory(&self.path);

        for track in &mut wire_asset.tracks {
            track.path = directory.relative(&track.path);
        }

        let bytes =
            serde_json::to_vec_pretty(&wire_asset).map_err(|source| Error::EncodeDescriptor {
                path: self.path.clone(),
                source,
            })?;
        op.write(self.path.as_str(), bytes)
            .await
            .map_err(|source| Error::WriteDescriptor {
                path: self.path.clone(),
                source,
            })?;
        Ok(())
    }

    /// Return the track at `path`, probing and adding it when it is not yet
    /// present in the asset.
    ///
    /// # Errors
    /// Returns any format, storage, parsing, container, or codec error from
    /// probing a new track.
    pub async fn index_track(&mut self, op: &Operator, path: &str) -> Result<&mut Track, Error> {
        let resolved_path = asset_directory(&self.path).join(path).normalize();

        if let Some(index) = self
            .tracks
            .iter()
            .position(|track| track.path == resolved_path)
        {
            return Ok(&mut self.tracks[index]);
        }

        let track = Track::probe(op, path, &self.path).await?;
        let index = self.tracks.len();
        self.tracks.push(track);
        Ok(&mut self.tracks[index])
    }
}

fn asset_directory(path: &RelativePath) -> &RelativePath {
    path.parent().unwrap_or_else(|| RelativePath::new(""))
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}
