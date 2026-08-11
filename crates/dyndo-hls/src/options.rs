#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HlsOptions {
    /// Package raw WebVTT sources as CMAF `wvtt` segments rather than serving
    /// them as plain WebVTT documents.
    ///
    /// CMAF `wvtt` sources always remain CMAF. This is off by default because
    /// HLS players handle raw WebVTT widely and a WebVTT rendition needs no
    /// initialization segment.
    pub wvtt: bool,
}
