use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DashOptions {
    #[serde(default, alias = "c")]
    pub compact: bool,
    /// Split the manifest into a Period at each segment boundary.
    ///
    /// Off by default: a boundary only asks for a segment to start there, which
    /// says nothing about whether a client should treat what follows as a
    /// separate presentation.
    #[serde(default, alias = "mp")]
    pub multi_period: bool,
}
