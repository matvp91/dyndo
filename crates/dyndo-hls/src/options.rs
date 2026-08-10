use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HlsOptions {
    /// Serve text tracks as packaged CMAF `wvtt` segments rather than as plain
    /// WebVTT documents.
    ///
    /// Off by default because HLS players handle WebVTT most widely and a WebVTT
    /// rendition needs no initialization segment.
    #[serde(default)]
    pub wvtt: bool,
}
