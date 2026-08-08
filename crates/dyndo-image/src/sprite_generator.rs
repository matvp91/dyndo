//! Sprites cut from a video track.
//!
//! A thumbnail track is not a track dyndo stores: it is a tile of frames cut from a
//! video track at a fixed step, addressed by the presentation time its first cell
//! shows. A sprite's duration follows from its step as `cells * step`, so every one
//! covers the same span and a manifest can describe the whole track with one repeated
//! timeline entry.
//!
//! Two range reads fetch what a sprite needs — the track's initialization segment for
//! the decoder, and the one contiguous range holding every frame its cells show — and
//! the sprite is built entirely in memory.

use bytes::Bytes;
use dyndo_core::asset_descriptor::TrackKind;
use dyndo_core::segment::SegmentOptions;
use dyndo_core::track::{Track, TrackError};
use opendal::Operator;

use crate::decode::{self, DecodeError};
use crate::fragment::{Fragment, FragmentError};
use crate::image::{Image, ImageError};
use crate::window::Window;

#[derive(Debug, thiserror::Error)]
pub enum SpriteError {
    #[error(transparent)]
    Track(#[from] TrackError),
    #[error(transparent)]
    Fragment(#[from] FragmentError),
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error(transparent)]
    Image(#[from] ImageError),
    #[error("track {0} is not a video track")]
    NotVideo(String),
    #[error("the presentation does not reach {0}ms")]
    NotFound(u64),
    #[error("a cell falls outside the range read")]
    CellOutsideRange,
}

/// Cuts the sprite whose first thumbnail shows `time`, in milliseconds from the start
/// of the presentation.
///
/// `tile_size` thumbnails go in a row and in a column — a player reads it as the value
/// of the DASH-IF `thumbnail_tile` essential property, where `5` becomes `5x5`, and
/// divides the sprite by it to place a cell. `height` is the whole sprite's, in pixels,
/// so a thumbnail is a `tile_size`th of it and its width follows the source's aspect.
/// `step` is the milliseconds between one thumbnail and the next.
///
/// # Errors
///
/// Returns a [`SpriteError`] when the track is not video, when the presentation does
/// not reach `time`, or when a frame cannot be read, decoded, or encoded.
pub async fn generate(
    op: &Operator,
    track: &Track,
    tile_size: u32,
    height: u32,
    step: u32,
    time: u64,
) -> Result<Bytes, SpriteError> {
    let TrackKind::Video(video) = track.kind() else {
        return Err(SpriteError::NotVideo(track.id().to_string()));
    };
    let window =
        Window::new(track, tile_size * tile_size, step, time).ok_or(SpriteError::NotFound(time))?;

    // The only read segment options change is a subtitle document's packaging, and a
    // sprite is only ever cut from a video track — so which options a request asked for
    // delivery in says nothing about how these bytes are read.
    let options = SegmentOptions::default();
    let initialization = track.read_initialization(op, &options).await?;
    let media = track.read_range(op, &options, window.range.clone()).await?;

    let codec = track.codec().to_string();
    let image = Image::new(tile_size, height, (video.width, video.height));

    // Decoding a sprite's frames and encoding it is hundreds of milliseconds of CPU,
    // which on the caller's executor would stall every request sharing its thread.
    tokio::task::spawn_blocking(move || compose(&codec, &initialization, &media, &window, image))
        .await
        .expect("composing a sprite does not panic")
}

/// Decodes the frame each cell shows and lays them out into one image.
fn compose(
    codec: &str,
    initialization: &[u8],
    media: &[u8],
    window: &Window,
    mut image: Image,
) -> Result<Bytes, SpriteError> {
    let mut decoder = decode::decoder(codec, initialization)?;

    for (index, cell) in window.cells.iter().enumerate() {
        // A cell the presentation never reaches stays black. DASH-IF expects a trailing
        // sprite to be partly filled, and a player placing a cell by time never asks
        // for one of them.
        let Some(cell) = cell else { continue };
        let bytes = media
            .get(cell.segment.start as usize..cell.segment.end as usize)
            .ok_or(SpriteError::CellOutsideRange)?;
        let fragment = Fragment::read(bytes)?;
        let index = u32::try_from(index).expect("a sprite holds no more cells than its tile size");

        image.place(index, decoder.frame_at(&fragment, cell.time)?)?;
    }

    Ok(image.encode()?)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::window::Cell;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");
    const TILE_SIZE: u32 = 5;
    const HEIGHT: u32 = 900;
    const SOURCE: (u32, u32) = (1920, 1080);

    #[test]
    fn compose_refuses_a_cell_pointing_outside_the_bytes_read() {
        let initialization = fs::read(format!("{FIXTURES}/video_avc_1080.mp4")).unwrap();
        let window = Window {
            range: 0..10,
            cells: vec![Some(Cell {
                segment: 0..10,
                time: 0,
            })],
        };
        let image = Image::new(TILE_SIZE, HEIGHT, SOURCE);

        let error = compose("avc1.640028", &initialization, &[], &window, image).unwrap_err();

        assert!(matches!(error, SpriteError::CellOutsideRange), "{error}");
    }

    #[test]
    fn compose_refuses_a_codec_no_decoder_handles() {
        let initialization = fs::read(format!("{FIXTURES}/video_avc_1080.mp4")).unwrap();
        let window = Window {
            range: 0..0,
            cells: Vec::new(),
        };
        let image = Image::new(TILE_SIZE, HEIGHT, SOURCE);

        let error = compose("hvc1.1.6.L120.90", &initialization, &[], &window, image).unwrap_err();

        assert!(
            matches!(error, SpriteError::Decode(DecodeError::UnsupportedCodec(_))),
            "{error}"
        );
    }
}
