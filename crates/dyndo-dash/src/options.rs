use std::time::Duration;

/// Controls how dyndo derives a static DASH presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DashOptions {
    /// Minimum duration of CMAF video and audio delivery segments.
    pub min_segment_duration: Duration,
    /// Target duration of generated, strict sidecar-text delivery segments.
    pub text_segment_duration: Duration,
    /// Hoist identical representation data to their adaptation set.
    pub compact: bool,
    /// Split the presentation into a Period at each splice boundary.
    pub multi_period: bool,
}

impl Default for DashOptions {
    fn default() -> Self {
        Self {
            min_segment_duration: Duration::ZERO,
            text_segment_duration: Duration::from_secs(6),
            compact: false,
            multi_period: false,
        }
    }
}
