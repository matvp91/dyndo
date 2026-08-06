//! Writing a fragmented subtitle out as a CMAF track.
//!
//! One module per container, each taking the fragments
//! [`fragmenter`](crate::fragmenter) produced and deciding what its format makes
//! of them. [`wvtt`] is the only one so far.

pub mod wvtt;

/// What stops a fragmented subtitle from being written out: one error for the
/// whole direction rather than one per container, mirroring
/// [`UnpackError`](crate::demuxer::UnpackError) on the way back.
#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("subtitle covers no time")]
    Empty,
    #[error(transparent)]
    Atom(#[from] mp4_atom::Error),
}
