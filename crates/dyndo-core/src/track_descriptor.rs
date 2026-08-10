use relative_path::RelativePathBuf;
use serde::{Deserialize, Serialize};

use super::track::Track;
use super::track_kind::TrackKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackDescriptor {
    pub id: String,
    /// Path relative to the asset descriptor; resolve it through
    /// [`super::asset_descriptor::AssetDescriptor::track_path`].
    pub(super) path: RelativePathBuf,
    pub codec: String,
    #[serde(flatten)]
    pub kind: TrackKind,
}

impl TrackDescriptor {
    pub(super) fn from_track(track: &Track, path: RelativePathBuf) -> Self {
        Self {
            id: track.id().to_string(),
            path,
            codec: track.codec().rfc6381(),
            kind: track.kind().clone(),
        }
    }
}
