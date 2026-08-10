//! One frame of a video track, encoded as a still.
//!
//! Two range reads produce it — the track's initialization segment for the decoder, and
//! the one segment holding the frame — and it comes out at the size of the track it is
//! cut from, so nothing here states its pixels.

use bytes::Bytes;
use dyndo_core::asset_descriptor::TrackKind;
use dyndo_core::frame_reader::{FrameReader, FrameReaderError};
use dyndo_core::segment::{self, Segment, SegmentOptions};
use dyndo_core::track::{Track, TrackError};
use image::RgbImage;
use image::codecs::jpeg::JpegEncoder;
use opendal::Operator;

use crate::next::decoder::{Decoder, DecoderError, Picture};

/// Quality a still is encoded at. It is shown at the size it was decoded rather than
/// downscaled into a cell, so it keeps detail a sprite has nowhere to put.
const QUALITY: u8 = 90;

#[derive(Debug, thiserror::Error)]
pub enum ImageReaderError {
    #[error(transparent)]
    Track(#[from] TrackError),
    #[error(transparent)]
    Frames(#[from] FrameReaderError),
    #[error(transparent)]
    Decode(#[from] DecoderError),
    #[error("track {0} is not a video track")]
    NotVideo(String),
    #[error("cannot decode codec {0}")]
    UnsupportedCodec(String),
    #[error("the presentation does not reach {0}ms")]
    NotFound(u64),
    #[error("decoded picture does not fill its buffer")]
    Picture,
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

    let raw_time = raw_time(track, time);
    let segment = locate(track, raw_time).ok_or(ImageReaderError::NotFound(time))?;

    // The only read segment options change is a subtitle document's packaging, and a
    // still is only ever cut from a video track — so which options a request asked for
    // delivery in says nothing about how these bytes are read.
    let options = SegmentOptions::default();
    let initialization = track.read_initialization(op, &options).await?;
    let media = track.read_range(op, &options, segment.byte_range()).await?;

    // Decoding a frame and encoding it is tens of milliseconds of CPU, which on the
    // caller's executor would stall every request sharing its thread.
    tokio::task::spawn_blocking(move || {
        let mut decoder = Decoder::new(&initialization)?;
        let frames = FrameReader::read(&media)?;

        encode(decoder.frame_at(&frames, raw_time)?)
    })
    .await
    .expect("decoding a still does not panic")
}

/// A presentation time in milliseconds, counted in the track's own timescale from where
/// its presentation begins.
///
/// Rounded down, so a still shows the frame on screen at the time it asks for rather
/// than the one after it.
fn raw_time(track: &Track, at_ms: u64) -> u64 {
    let raw = u128::from(at_ms) * u128::from(track.timescale()) / 1000;

    track
        .earliest_presentation_time()
        .saturating_add(u64::try_from(raw).unwrap_or(u64::MAX))
}

/// The segment holding the frame shown at `raw_time`, or `None` once the presentation
/// ends before it.
///
/// Default options group nothing, so this is one stored fragment: what a still shows
/// must not shift with the segmentation a request asks for delivery in.
fn locate(track: &Track, raw_time: u64) -> Option<Segment> {
    segment::segments(track, &SegmentOptions::default())
        .into_iter()
        .find(|segment| segment.time_range().contains(&raw_time))
}

fn encode(picture: Picture) -> Result<Bytes, ImageReaderError> {
    let decoded = RgbImage::from_raw(picture.width, picture.height, picture.rgb)
        .ok_or(ImageReaderError::Picture)?;
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

    /// The fixture declares 715 fragments of 1.92s at timescale 90000, so the frame
    /// shown at 10s is the 900_000th unit of its clock, cut from the sixth fragment.
    #[tokio::test]
    async fn a_still_is_cut_from_the_fragment_holding_the_time_it_shows() {
        let (_, track) = probe("video_avc_1080.mp4").await;
        let segments = segment::segments(&track, &SegmentOptions::default());

        let raw_time = raw_time(&track, 10_000);

        assert_eq!(
            (raw_time, locate(&track, raw_time)),
            (900_000, Some(segments[5]))
        );
    }

    /// A still is the size of the track it is cut from, which is what a decoded picture
    /// filling its buffer comes out as.
    #[test]
    fn encode_writes_a_jpeg_the_size_of_the_picture() {
        let encoded = encode(Picture {
            width: 16,
            height: 8,
            rgb: vec![255; 16 * 8 * 3],
        })
        .unwrap();

        let decoded = image::load_from_memory_with_format(&encoded, ImageFormat::Jpeg).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (16, 8));
    }

    #[test]
    fn encode_refuses_a_picture_that_does_not_fill_its_buffer() {
        let error = encode(Picture {
            width: 16,
            height: 16,
            rgb: vec![0; 3],
        })
        .unwrap_err();

        assert!(matches!(error, ImageReaderError::Picture), "{error}");
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
