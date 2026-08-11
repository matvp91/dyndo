use relative_path::RelativePathBuf;
use serde::{Deserialize, Serialize};

use crate::track::kind::TextKind;

/// A timed-text source track configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimedTextTrackDescriptor {
    pub id: String,
    /// Path relative to the asset descriptor.
    pub(super) path: RelativePathBuf,
    #[serde(flatten)]
    pub kind: TextKind,
}
