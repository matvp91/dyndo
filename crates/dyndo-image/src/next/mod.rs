//! The version of this crate that reads its frames from `dyndo-core`.
//!
//! Everything here is a rewrite of the module beside it, free to break from what the
//! old one settled on: [`decoder`] takes the frames [`dyndo_core::frame_reader`] read
//! rather than parsing a fragment itself, so nothing in `next` answers a question about
//! the container. It stands alongside the old modules until it replaces them, and the
//! old ones are then scratched.

pub mod decoder;
pub mod image_reader;
