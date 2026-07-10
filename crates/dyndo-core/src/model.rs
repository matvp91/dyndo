use opendal::Operator;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetModel {
    pub tracks: Vec<TrackModel>,
}

impl AssetModel {
    pub async fn read(op: &Operator, path: &str) -> Result<AssetModel, Box<dyn std::error::Error>> {
        let bytes = op.read(path).await?;
        Ok(serde_json::from_slice(&bytes.to_vec())?)
    }

    /// Serialize to pretty JSON and write to `path` through `op`.
    pub async fn write(&self, op: &Operator, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let bytes = serde_json::to_vec_pretty(self)?;
        op.write(path, bytes).await?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TrackModel {
    Video(VideoTrackModel),
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoTrackModel {
    pub id: String,
    pub path: String,
    pub fourcc: String,
    pub timescale: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioTrackModel {
    pub id: String,
    pub path: String,
    pub fourcc: String,
    pub timescale: u32,
    pub sample_rate: u32,
    pub channels: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}
