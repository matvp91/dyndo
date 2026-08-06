//! Reading a served segment of a CMAF text track back into a subtitle.
//!
//! The mirror of [`muxer`](crate::muxer): one module per container, each taking
//! the bytes a segment resolves to and recovering the cues it carries. [`wvtt`]
//! is the only one so far.

pub mod wvtt;

/// What stops a served segment from being read back: one error for the whole
/// direction rather than one per container.
#[derive(Debug, thiserror::Error)]
pub enum UnpackError {
    #[error("fragment carries no base decode time")]
    MissingBaseTime,
    #[error("fragment carries no sample durations")]
    MissingSampleTiming,
    #[error("sample data overruns the fragment")]
    SampleOutOfRange,
    #[error("a fragment header and its sample data do not pair up")]
    UnpairedFragment,
    #[error("time {0} does not fit in the milliseconds a cue counts")]
    TimeOverflow(u64),
    #[error(transparent)]
    Atom(#[from] mp4_atom::Error),
}
