use serde::{Deserialize, Serialize};

/// A synthetic track configuration in an asset descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntheticTrackDescriptor<K> {
    /// Identifier used to address the synthetic track.
    pub id: String,
    #[serde(flatten)]
    pub kind: K,
}
