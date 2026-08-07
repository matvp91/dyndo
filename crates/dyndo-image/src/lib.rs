//! Thumbnail sprite sheets, cut from a video track as they are asked for.
//!
//! Nothing is stored and nothing is written: a sheet is built from the CMAF the
//! asset already points at, with two range reads and no temporary files. The first
//! read is the track's initialization segment, for the AVC parameter sets a frame
//! needs in order to decode on its own; the second is the one contiguous range
//! holding every fragment the sheet's cells fall in.
//!
//! Three modules, one per thing being made: [`Sprite`](sprite::Sprite) decides which
//! frames a sheet shows and fetches the bytes holding them, `avc_decoder` decodes the
//! keyframe each of those fragments opens on, and `image` scales the frames into the
//! sheet and encodes it.
//!
//! AVC only, since that is what openh264 decodes; anything else is refused rather
//! than guessed at.

mod avc_decoder;
mod image;
pub mod sprite;

use dyndo_core::track::TrackError;

#[derive(Debug, thiserror::Error)]
pub enum ThumbnailError {
    #[error(transparent)]
    Track(#[from] TrackError),
    #[error("malformed track container: {0}")]
    Parse(#[from] mp4_atom::Error),
    #[error("invalid track container: {0}")]
    Container(&'static str),
    #[error("decoding failed: {0}")]
    Decode(#[from] openh264::Error),
    #[error("encoding the sheet failed: {0}")]
    Encode(#[from] ::image::ImageError),
    #[error("track {0} is not a video track")]
    NotVideo(String),
    #[error("cannot cut thumbnails from codec {0}")]
    UnsupportedCodec(String),
    #[error("no sprite sheet starts at {0}ms")]
    NotFound(u64),
    #[error("fragment does not open on a keyframe")]
    NoKeyframe,
    #[error("no picture decoded for a fragment's keyframe")]
    EmptyFrame,
}
