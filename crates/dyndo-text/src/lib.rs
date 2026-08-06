//! Subtitles for dyndo: a source document in, a CMAF text track out.
//!
//! [`vtt`] parses a document into a [`Subtitle`](subtitle::Subtitle),
//! [`fragmenter`] divides one into the fragments a track is built from, and a
//! [`muxer`] writes those fragments out in a container.

pub mod fragmenter;
pub mod muxer;
pub mod subtitle;
pub mod vtt;
