use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentOptions {
    /// The shortest a served segment may be, in milliseconds; fragments are
    /// grouped until they reach it.
    #[serde(default, alias = "sml", alias = "segment_min_length")]
    pub min_length: u32,
    /// How long each segment of a packaged subtitle track is, in milliseconds.
    /// Unlike `min_length` this is exact, since dyndo fragments those tracks
    /// itself rather than grouping what a file already contains. Zero asks for no
    /// grid, leaving the asset's splice points as the only cuts.
    #[serde(default, alias = "stl", alias = "segment_text_length")]
    pub text_length: u32,
    /// Times a segment has to start at, in milliseconds.
    #[serde(default, alias = "sb", alias = "segment_boundaries")]
    pub boundaries: Vec<u32>,
}
