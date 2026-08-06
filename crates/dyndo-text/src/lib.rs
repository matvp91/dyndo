//! Subtitles for dyndo: a source document in, a CMAF text track out — and back.
//!
//! Three transformations, each a module owning both of its directions:
//!
//! - [`vtt`] parses a WebVTT document into a [`Subtitle`](subtitle::Subtitle) and
//!   writes one back out;
//! - [`fragmenter`] divides a subtitle into the fragments a track is built from, and
//!   merges those fragments back into a subtitle;
//! - [`wvtt`] packs fragments into a CMAF container and unpacks a served segment of
//!   one.
//!
//! Which is why a transport can ask for the cues rather than the container: unpack a
//! segment, merge it, and write it out.

pub mod fragmenter;
pub mod subtitle;
pub mod vtt;
pub mod wvtt;
