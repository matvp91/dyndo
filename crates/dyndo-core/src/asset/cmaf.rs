use relative_path::RelativePathBuf;
use serde::{Deserialize, Serialize};

/// A CMAF source track configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CmafTrackDescriptor<K> {
    pub id: String,
    /// Path relative to the asset descriptor.
    pub(super) path: RelativePathBuf,
    pub codec: String,
    #[serde(flatten)]
    pub kind: K,
}
