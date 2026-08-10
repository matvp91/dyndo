#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HlsOptions {
    /// Serve text tracks as packaged CMAF `wvtt` segments rather than as plain
    /// WebVTT documents.
    ///
    /// Off by default because HLS players handle WebVTT most widely and a WebVTT
    /// rendition needs no initialization segment.
    pub wvtt: bool,
}
