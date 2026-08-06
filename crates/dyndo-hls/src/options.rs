use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HlsOptions {
    /// Serve text tracks as packaged CMAF `wvtt` segments rather than as plain
    /// WebVTT documents.
    ///
    /// Off by default: HLS players handle WebVTT most widely, and a WebVTT
    /// rendition needs no initialization segment. Ask for `wvtt` when a client
    /// wants the packaged track, or when a text track came from a `wvtt` file
    /// another packager wrote and so cannot be unpacked.
    #[serde(default)]
    pub wvtt: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wvtt_defaults_to_false() {
        assert!(!HlsOptions::default().wvtt);
    }
}
