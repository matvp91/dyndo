//! The wire model (`asset.json`): serializable [`AssetModel`] and [`TrackModel`].

use bytes::Buf;
use opendal::Operator;
use serde::{Deserialize, Serialize};

use crate::CoreError;

/// `skip_serializing_if` helper: the wire omits a zero `min_segment_length`.
fn is_zero(v: &u64) -> bool {
    *v == 0
}

/// The serializable descriptor (`asset.json`): a list of tracks plus the
/// optional serve-time segmentation policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetModel {
    /// Minimum length of a served segment, in milliseconds (wire:
    /// `min_segment_length`). `0` (or an absent field — deserialization
    /// normalizes the two) serves each CMAF fragment as its own segment.
    #[serde(
        rename = "min_segment_length",
        default,
        skip_serializing_if = "is_zero"
    )]
    pub min_segment_length_ms: u64,
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
    /// A text track, with language.
    Text(TextTrackModel),
}

impl TrackModel {
    /// The representation id recorded in the descriptor.
    pub fn id(&self) -> &str {
        match self {
            TrackModel::Video(v) => &v.id,
            TrackModel::Audio(a) => &a.id,
            TrackModel::Text(t) => &t.id,
        }
    }

    /// The track's source path, relative to the descriptor.
    pub fn path(&self) -> &str {
        match self {
            TrackModel::Video(v) => &v.path,
            TrackModel::Audio(a) => &a.path,
            TrackModel::Text(t) => &t.path,
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
    /// Number of audio channels (e.g. 2 for stereo, 6 for 5.1).
    pub channels: u16,
    /// ISO-639-2 language code; omitted from the JSON when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// The text-track fields of the wire model (`asset.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextTrackModel {
    /// Representation id (see [`TrackModel::id`]).
    pub id: String,
    /// Source path of the track file, relative to the descriptor.
    pub path: String,
    /// Sample-entry fourcc (e.g. `"wvtt"`).
    pub fourcc: String,
    /// Units per second for durations in this track.
    pub timescale: u32,
    /// ISO-639-2 language code (`"und"` when unspecified).
    pub language: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_without_grouping_fields_parses_with_defaults() {
        let m: AssetModel = serde_json::from_str(r#"{"tracks": []}"#).unwrap();
        assert_eq!(m.min_segment_length_ms, 0);
        assert!(m.segment_boundaries_ms.is_empty());
    }

    #[test]
    fn grouping_fields_parse_from_their_wire_names_in_ms() {
        let m: AssetModel = serde_json::from_str(
            r#"{"min_segment_length": 3000, "segment_boundaries": [683640], "tracks": []}"#,
        )
        .unwrap();
        assert_eq!(m.min_segment_length_ms, 3000);
        assert_eq!(m.segment_boundaries_ms, vec![683640]);
    }

    #[test]
    fn grouping_fields_round_trip_through_json() {
        let m = AssetModel {
            min_segment_length_ms: 3000,
            segment_boundaries_ms: vec![683640],
            tracks: Vec::new(),
        };
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(serde_json::from_str::<AssetModel>(&json).unwrap(), m);
    }

    #[test]
    fn default_grouping_fields_are_omitted_from_json() {
        let m = AssetModel {
            min_segment_length_ms: 0,
            segment_boundaries_ms: Vec::new(),
            tracks: Vec::new(),
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(!json.contains("min_segment_length"));
        assert!(!json.contains("segment_boundaries"));
    }
}
