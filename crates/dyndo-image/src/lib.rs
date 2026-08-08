//! Thumbnail sprites, cut from a video track as they are asked for.
//!
//! Nothing is stored and nothing is written: a sprite is built from the CMAF the asset
//! already points at, with two range reads and no temporary files.
//!
//! Four modules, one responsibility each:
//!
//! - [`sprite`] is the sprite asked for — its parameters, and the reads and work that
//!   produce it;
//! - [`fragment`] answers which bytes of a fragment are a frame and what time it is
//!   shown at, which is a question about the container rather than the codec;
//! - [`decode`] turns those samples into a picture, and is where a second codec would
//!   be added;
//! - [`image`] scales the pictures into the sprite and encodes it.

pub mod decode;
pub mod fragment;
pub mod image;
pub mod sprite;
mod window;
