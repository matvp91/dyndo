//! Subtitles for dyndo: a source document in, a CMAF text track out — and back.
//!
//! [`vtt`] parses a document into a [`Subtitle`](subtitle::Subtitle),
//! [`fragmenter`] divides one into the fragments a track is built from, and a
//! [`muxer`] writes those fragments out in a container. A [`demuxer`] reads one
//! served segment of such a track back into a subtitle, which
//! [`Subtitle::write`](subtitle::Subtitle::write) turns into a document again —
//! the path a transport takes when it wants the cues rather than the container.

mod atoms;
pub mod demuxer;
pub mod fragmenter;
pub mod muxer;
pub mod subtitle;
pub mod vtt;
