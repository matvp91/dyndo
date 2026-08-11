use relative_path::RelativePathBuf;
use serde::{Deserialize, Serialize};

use crate::track::cmaf::kind::TextKind;

/// A raw WebVTT source track configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebVttTrackDescriptor {
    pub id: String,
    /// Path relative to the asset descriptor.
    pub(super) path: RelativePathBuf,
    #[serde(flatten)]
    pub kind: TextKind,
}
