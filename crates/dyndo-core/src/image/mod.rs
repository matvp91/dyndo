//! Video frame extraction and image encoding.

mod frame_grab;
mod sprite;

pub use frame_grab::{FrameGrab, FrameGrabError};
pub use sprite::{Sprite, SpriteError};
