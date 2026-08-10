mod partition;
mod policy;
mod segmenter;

pub(crate) use partition::partition;
pub use policy::{DurationPolicy, SegmentationPolicy};
pub use segmenter::Segmenter;
