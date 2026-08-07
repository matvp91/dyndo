//! Turning a fragment's samples into a picture.
//!
//! One codec is implemented, [`avc`]. The trait is what a second would have to
//! satisfy: configured once from a track's initialization segment, then asked for the
//! frame shown at a time. [`decoder`] is the only place a codec string is matched, so
//! it is the one place another decoder has to be named.

pub mod avc;

use crate::fragment::{Fragment, FragmentError};

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error(transparent)]
    Fragment(#[from] FragmentError),
    #[error("decoding failed: {0}")]
    Decoder(#[from] openh264::Error),
    #[error("invalid coded stream: {0}")]
    Stream(&'static str),
    #[error("cannot decode codec {0}")]
    UnsupportedCodec(String),
    #[error("no picture decoded for the frame asked for")]
    EmptyFrame,
}

/// One decoded picture, as packed 8-bit RGB.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

/// What a decoder offers: the frame shown at a time, out of the fragment holding it.
pub trait Decode {
    /// Decodes the frame shown at `time`, in the track's timescale — the clock the
    /// fragment stamps its samples on.
    ///
    /// # Errors
    ///
    /// Returns a [`DecodeError`] when the coded stream cannot be decoded.
    fn frame_at(&mut self, fragment: &Fragment<'_>, time: u64) -> Result<Frame, DecodeError>;
}

/// The decoder for a track of `codec`, configured from that track's initialization
/// segment.
///
/// # Errors
///
/// Returns a [`DecodeError`] when no decoder here handles `codec`, or when the segment
/// does not describe the track it claims to.
pub fn decoder(codec: &str, initialization: &[u8]) -> Result<Box<dyn Decode>, DecodeError> {
    if codec.starts_with(avc::SAMPLE_ENTRY) {
        return Ok(Box::new(avc::Decoder::new(initialization)?));
    }

    Err(DecodeError::UnsupportedCodec(codec.to_string()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");

    #[test]
    fn decoder_configures_avc_from_its_initialization_segment() {
        let initialization = fs::read(format!("{FIXTURES}/video_avc_1080.mp4")).unwrap();

        assert!(decoder("avc1.640028", &initialization).is_ok());
    }

    /// Which codecs can be decoded is the dispatch's to know, so a track nothing here
    /// handles is refused by name rather than guessed at.
    #[test]
    fn decoder_refuses_a_codec_no_decoder_handles() {
        let initialization = fs::read(format!("{FIXTURES}/video_avc_1080.mp4")).unwrap();

        let Err(error) = decoder("hvc1.1.6.L120.90", &initialization) else {
            panic!("a codec no decoder handles unexpectedly built one");
        };

        assert!(
            matches!(error, DecodeError::UnsupportedCodec(codec) if codec == "hvc1.1.6.L120.90"),
        );
    }
}
