//! The wire model (`asset.json`): serializable [`AssetModel`] and [`TrackModel`].

use bytes::Buf;
use opendal::Operator;
use serde::{Deserialize, Serialize};

use crate::CoreError;

/// The serializable descriptor (`asset.json`): a list of tracks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetModel {
    /// The asset's tracks, in descriptor order.
    pub tracks: Vec<TrackModel>,
}

impl AssetModel {
    /// Read and deserialize the descriptor JSON at `path` through `op`.
    ///
    /// # Errors
    /// [`CoreError::Storage`] if the object is missing or unreadable;
    /// [`CoreError::Descriptor`] if the bytes are not valid descriptor JSON.
    pub async fn read(op: &Operator, path: &str) -> Result<AssetModel, CoreError> {
        let buf = op.read(path).await?;
        Ok(serde_json::from_reader(buf.reader())?)
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
}

/// One track's wire representation, tagged by media type
/// (`"type": "video"|"audio"`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TrackModel {
    /// A video track, with dimensions and codec.
    Video(VideoTrackModel),
    /// An audio track, with sample rate, channels, and language.
    Audio(AudioTrackModel),
}

impl TrackModel {
    /// The representation id recorded in the descriptor.
    pub fn id(&self) -> &str {
        match self {
            TrackModel::Video(v) => &v.id,
            TrackModel::Audio(a) => &a.id,
        }
    }

    /// The track's source path, relative to the descriptor.
    pub fn path(&self) -> &str {
        match self {
            TrackModel::Video(v) => &v.path,
            TrackModel::Audio(a) => &a.path,
        }
    }
}

/// The video-track fields of the wire model (`asset.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoTrackModel {
    /// Representation id (see [`TrackModel::id`]).
    pub id: String,
    /// Source path of the track file, relative to the descriptor.
    pub path: String,
    /// Sample-entry fourcc (e.g. `"avc1"`).
    pub fourcc: String,
    /// Units per second for durations in this track.
    pub timescale: u32,
    /// Visual width, in pixels.
    pub width: u32,
    /// Visual height, in pixels.
    pub height: u32,
}

/// The audio-track fields of the wire model (`asset.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioTrackModel {
    /// Representation id (see [`TrackModel::id`]).
    pub id: String,
    /// Source path of the track file, relative to the descriptor.
    pub path: String,
    /// Sample-entry fourcc (e.g. `"mp4a"`, `"ac-3"`, `"ec-3"`).
    pub fourcc: String,
    /// Units per second for durations in this track.
    pub timescale: u32,
    /// Sampling rate, in Hz.
    pub sample_rate: u32,
    /// Channel count.
    pub channels: u16,
    /// ISO-639-2 language code; omitted from the JSON when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}
