//! The `Asset` is the descriptor (`asset.json`) model: it deserializes from
//! and serializes to the wire directly, with runtime-only fields skipped.

use bytes::Buf;
use futures_util::future::try_join_all;
use opendal::Operator;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::track::Track;

/// A dyndo asset: its tracks and where the descriptor was sourced from.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Asset {
    /// Splice points, in milliseconds from the start of the presentation
    /// (wire: `segment_boundaries`). Served segments never span one. Treated
    /// as a set: order and duplicates don't matter.
    #[serde(
        rename = "segment_boundaries",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub segment_boundaries_ms: Vec<u64>,
    /// The asset's tracks, in descriptor order.
    pub tracks: Vec<Track>,
    /// Path of the source descriptor (`asset.json`), used to resolve each
    /// track's relative path. Never on the wire; set by [`Asset::read`].
    #[serde(skip)]
    pub path: String,
}

impl Asset {
    /// An empty asset: no tracks, empty source path.
    pub fn new() -> Asset {
        Asset::default()
    }

    /// Read and deserialize the descriptor JSON at `path` through `op`,
    /// recording `path` as the asset's source and probing every track's
    /// header.
    ///
    /// # Errors
    /// [`CoreError::Storage`] if the object is missing or unreadable;
    /// [`CoreError::Descriptor`] if the bytes are not valid descriptor JSON;
    /// otherwise any [`CoreError`] from probing a track's header.
    pub async fn read(op: &Operator, path: &str) -> Result<Asset, CoreError> {
        let buf = op.read(path).await?;
        let mut asset: Asset = serde_json::from_reader(buf.reader())?;
        asset.path = path.to_string();
        // Tracks are independent, so all headers are probed concurrently;
        // each read resolves the track's descriptor-relative path itself.
        try_join_all(asset.tracks.iter_mut().map(|t| t.read_header(op, path))).await?;
        Ok(asset)
    }

    /// Serialize to pretty JSON and write to `path` through `op`.
    ///
    /// # Errors
    /// [`CoreError::Descriptor`] if serialization fails; [`CoreError::Storage`]
    /// if the write fails.
    pub async fn write(&self, op: &Operator, path: &str) -> Result<(), CoreError> {
        let bytes = serde_json::to_vec_pretty(self)?;
        op.write(path, bytes).await?;
        Ok(())
    }

    /// The asset's presentation duration, in milliseconds: the longest
    /// track's duration.
    ///
    /// # Panics
    /// If a track has not been probed.
    pub fn duration_ms(&self) -> u64 {
        self.tracks
            .iter()
            .map(Track::duration_ms)
            .max()
            .unwrap_or(0)
    }

    /// The longest (sub)segment across the asset's tracks, in milliseconds
    /// (`0` if it has none).
    ///
    /// # Panics
    /// If a track has not been probed.
    pub fn max_segment_duration_ms(&self) -> u64 {
        self.tracks
            .iter()
            .map(Track::max_segment_duration_ms)
            .max()
            .unwrap_or(0)
    }
}
