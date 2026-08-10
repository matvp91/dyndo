use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationPolicy {
    Exact(u32),
    Minimum(u32),
}

impl DurationPolicy {
    pub fn duration(self) -> u32 {
        match self {
            Self::Exact(duration) | Self::Minimum(duration) => duration,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentationPolicy {
    boundaries: Arc<[u32]>,
    duration: DurationPolicy,
}

impl SegmentationPolicy {
    pub fn new(boundaries: &[u32], duration: DurationPolicy) -> Self {
        Self {
            boundaries: boundaries.into(),
            duration,
        }
    }

    pub fn boundaries(&self) -> &[u32] {
        &self.boundaries
    }

    pub fn duration(&self) -> DurationPolicy {
        self.duration
    }
}
