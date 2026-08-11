use relative_path::RelativePathBuf;
use serde::{Deserialize, Serialize};

use super::kind::{AudioKind, TextKind, ThumbnailKind, VideoKind};

/// A video CMAF source track configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoTrackDescriptor {
    pub id: String,
    /// Path relative to the asset descriptor.
    pub(super) path: RelativePathBuf,
    pub codec: String,
    #[serde(flatten)]
    pub kind: VideoKind,
}

/// An audio CMAF source track configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioTrackDescriptor {
    pub id: String,
    /// Path relative to the asset descriptor.
    pub(super) path: RelativePathBuf,
    pub codec: String,
    #[serde(flatten)]
    pub kind: AudioKind,
}

/// A text CMAF source track configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextTrackDescriptor {
    pub id: String,
    /// Path relative to the asset descriptor.
    pub(super) path: RelativePathBuf,
    pub codec: String,
    #[serde(flatten)]
    pub kind: TextKind,
}

/// A raw WebVTT source track configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebVttTrackDescriptor {
    pub id: String,
    /// Path relative to the asset descriptor.
    pub(super) path: RelativePathBuf,
    #[serde(flatten)]
    pub kind: TextKind,
}

/// A thumbnail synthetic track configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThumbnailTrackDescriptor {
    /// Identifier used to address the synthetic track.
    pub id: String,
    #[serde(flatten)]
    pub kind: ThumbnailKind,
}
