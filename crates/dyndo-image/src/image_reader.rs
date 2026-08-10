//! One frame of a video track, encoded as a still.
//!
//! A still is a sprite of a single cell: the same frame is found the same way and
//! decoded by the same decoder, then encoded on its own rather than laid out in a grid.
//! It comes out at the size of the track it is cut from, so nothing here states its
//! pixels either.

use bytes::Bytes;
use dyndo_core::asset_descriptor::TrackKind;
use dyndo_core::segment::SegmentOptions;
use dyndo_core::track::{Track, TrackError};
use image::RgbImage;
use image::codecs::jpeg::JpegEncoder;
use opendal::Operator;

use crate::decoder::{Decoder, DecoderError, Frame};
use crate::fragment::{Fragment, FragmentError};
use crate::window::Window;

/// Quality a still is encoded at. It is shown at the size it was decoded rather than
/// downscaled into a cell, so it keeps detail a sprite has nowhere to put.
const QUALITY: u8 = 90;

#[derive(Debug, thiserror::Error)]
pub enum ImageReaderError {
    #[error(transparent)]
    Track(#[from] TrackError),
    #[error(transparent)]
    Fragment(#[from] FragmentError),
    #[error(transparent)]
    Decode(#[from] DecoderError),
    #[error("track {0} is not a video track")]
    NotVideo(String),
    #[error("cannot decode codec {0}")]
    UnsupportedCodec(String),
    #[error("the presentation does not reach {0}ms")]
    NotFound(u64),
    #[error("decoded frame does not fill its buffer")]
    Frame,
    #[error("encoding the still failed: {0}")]
    Encode(#[from] image::ImageError),
}

/// Reads the frame `track` shows at `time` — milliseconds from the start of the
/// presentation — and encodes it as a JPEG.
///
/// # Errors
///
/// Returns an [`ImageReaderError`] when the track is not video, when it is coded any
/// way but AVC — the one codec [`Decoder`] reads — when the presentation does not reach
/// `time`, or when the frame cannot be read, decoded, or encoded.
pub async fn read(op: &Operator, track: &Track, time: u64) -> Result<Bytes, ImageReaderError> {
    let TrackKind::Video(_) = track.kind() else {
        return Err(ImageReaderError::NotVideo(track.id().to_string()));
    };
    if !track.codec().starts_with("avc1") {
        return Err(ImageReaderError::UnsupportedCodec(
            track.codec().to_string(),
        ));
    }

    // A still is one cell of a sprite, so it is located the same way: a window of a
    // single thumbnail, whose step has nothing to advance.
    let window = Window::new(track, 1, 1, time).ok_or(ImageReaderError::NotFound(time))?;
    let cell = window
        .cells
        .into_iter()
        .flatten()
        .next()
        .expect("a window the presentation reaches holds its cell");
    let raw_time = cell.time;

    // The only read segment options change is a subtitle document's packaging, and a
    // still is only ever cut from a video track — so which options a request asked for
    // delivery in says nothing about how these bytes are read.
    let options = SegmentOptions::default();
    let initialization = track.read_initialization(op, &options).await?;
    let media = track.read_range(op, &options, cell.segment).await?;

    // Decoding a frame and encoding it is tens of milliseconds of CPU, which on the
    // caller's executor would stall every request sharing its thread.
    tokio::task::spawn_blocking(move || {
        let mut decoder = Decoder::new(&initialization)?;
        let fragment = Fragment::read(&media)?;

        encode(decoder.frame_at(&fragment, raw_time)?)
    })
    .await
    .expect("decoding a still does not panic")
}

fn encode(frame: Frame) -> Result<Bytes, ImageReaderError> {
    let decoded =
        RgbImage::from_raw(frame.width, frame.height, frame.rgb).ok_or(ImageReaderError::Frame)?;
    let mut encoded = Vec::new();
    JpegEncoder::new_with_quality(&mut encoded, QUALITY).encode_image(&decoded)?;

    Ok(Bytes::from(encoded))
}

#[cfg(test)]
mod tests {
    use image::ImageFormat;
    use opendal::services::Memory;
    use relative_path::RelativePath;

    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");

    #[tokio::test]
    async fn read_refuses_a_track_that_is_not_video() {
        let (op, track) = probe("audio_aac_nl_2.mp4").await;

        let error = read(&op, &track, 0).await.unwrap_err();

        assert!(matches!(error, ImageReaderError::NotVideo(_)), "{error}");
    }

    /// A still is addressed by the time it shows, so a time past the end of the
    /// presentation names no frame at all.
    #[tokio::test]
    async fn read_refuses_a_time_the_presentation_never_reaches() {
        let (op, track) = probe("video_avc_1080.mp4").await;

        let error = read(&op, &track, 1_400_000).await.unwrap_err();

        assert!(matches!(error, ImageReaderError::NotFound(_)), "{error}");
    }

    /// A still is the size of the track it is cut from, which is what a decoded frame
    /// filling its buffer comes out as.
    #[test]
    fn encode_writes_a_jpeg_the_size_of_the_frame() {
        let encoded = encode(Frame {
            width: 16,
            height: 8,
            rgb: vec![255; 16 * 8 * 3],
        })
        .unwrap();

        let decoded = image::load_from_memory_with_format(&encoded, ImageFormat::Jpeg).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (16, 8));
    }

    #[test]
    fn encode_refuses_a_frame_that_does_not_fill_its_buffer() {
        let error = encode(Frame {
            width: 16,
            height: 16,
            rgb: vec![0; 3],
        })
        .unwrap_err();

        assert!(matches!(error, ImageReaderError::Frame), "{error}");
    }

    async fn probe(name: &str) -> (Operator, Track) {
        let op = Operator::new(Memory::default()).unwrap();
        op.write(name, std::fs::read(format!("{FIXTURES}/{name}")).unwrap())
            .await
            .unwrap();
        let track = Track::probe(
            &op,
            RelativePath::new(name),
            None,
            &SegmentOptions::default(),
        )
        .await
        .unwrap();

        (op, track)
    }
}
