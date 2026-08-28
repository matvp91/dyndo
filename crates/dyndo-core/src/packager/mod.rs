//! Build CMAF tracks from a format implementation and media segments.

#[path = "packager.rs"]
mod implementation;
mod media_segment;

pub mod wvtt;

pub use implementation::{Format, Packager};
pub use media_segment::{MediaSegment, Sample};

#[derive(Debug, thiserror::Error)]
pub enum PackagerError {
    #[error("track ID must not be zero")]
    InvalidTrackId,
    #[error("timescale must not be zero")]
    InvalidTimescale,
    #[error("media covers no time")]
    Empty,
    #[error("a sample is too large")]
    SampleTooLarge,
    #[error("a media segment is too large")]
    MediaSegmentTooLarge,
    #[error("a media segment duration overflows")]
    DurationOverflow,
    #[error("media contains too many media segments")]
    TooManyMediaSegments,
    #[error(transparent)]
    Atom(#[from] mp4_atom::Error),
}
