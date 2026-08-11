#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HlsOptions {
    /// Thumbnails per sprite row and column. Zero disables HLS image output.
    pub thumbnail_tile_size: u32,
    /// Milliseconds between adjacent thumbnails. Zero disables HLS image output.
    pub thumbnail_step: u32,
    /// Serve text tracks as packaged CMAF `wvtt` segments rather than as plain
    /// WebVTT documents.
    ///
    /// Off by default because HLS players handle WebVTT most widely and a WebVTT
    /// rendition needs no initialization segment.
    pub wvtt: bool,
}
