//! Video frame extraction and image encoding.

#[expect(
    dead_code,
    reason = "The proof of concept is intentionally not wired into thumbnail generation yet."
)]
pub(crate) mod experimental_sprite_generator;
mod frame_extractor;

pub use frame_extractor::{FrameExtractor, FrameExtractorError};
