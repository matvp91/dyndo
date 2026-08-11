//! Video frame extraction and image encoding.

mod frame_extractor;
mod thumbnail;

pub use frame_extractor::{FrameExtractor, FrameExtractorError};
pub use thumbnail::{Thumbnail, ThumbnailError};
