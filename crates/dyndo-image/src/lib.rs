//! Thumbnail sprites, cut from a video track as they are asked for.
//!
//! Nothing is stored and nothing is written: a sprite is built from the CMAF the asset
//! already points at, with two range reads and no temporary files.
//!
//! Four modules, one responsibility each:
//!
//! - [`sprite`] is the sprite asked for — its parameters, the reads and work that
//!   produce it, and the grid its frames are laid out in;
//! - [`image_reader`] is one frame of that same track, encoded on its own;
//! - [`fragment`] answers which bytes of a fragment are a frame and what time it is
//!   shown at, which is a question about the container rather than the codec;
//! - [`decoder`] turns those samples into a picture, and is the only module that knows
//!   a codec.
//!
//! [`next`] is where those modules are being rewritten against `dyndo-core`'s frame
//! reader, and replaces them once it holds everything they do.

pub mod decoder;
pub mod fragment;
pub mod image_reader;
pub mod next;
pub mod sprite;
mod window;
