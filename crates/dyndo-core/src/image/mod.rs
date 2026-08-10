//! Video frame extraction and image encoding.

mod frame_grab;
mod sprite_canvas;
mod thumbnail;

pub use frame_grab::{FrameGrab, FrameGrabError};
pub use thumbnail::{Thumbnail, ThumbnailError};
