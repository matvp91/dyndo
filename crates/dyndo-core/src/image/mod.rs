//! Video frame extraction and image encoding.

#[expect(
    dead_code,
    reason = "The proof of concept is intentionally not wired into thumbnail generation yet."
)]
pub(crate) mod experimental_sprite_generator;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "The proof of concept is intentionally not wired into thumbnail generation yet."
    )
)]
pub(crate) mod experimental_sprite_generator_2;
mod frame_extractor;

pub use frame_extractor::{FrameExtractor, FrameExtractorError};
