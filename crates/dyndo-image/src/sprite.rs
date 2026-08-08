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
//!
//! A sprite is a fixed grid of cells whatever it holds: a cell no frame was decoded
//! into stays black rather than shrinking the grid, so every sprite cut from an asset
//! comes out the size its manifest advertised.

use std::num::NonZero;

use bytes::Bytes;
use dyndo_core::asset_descriptor::TrackKind;
use dyndo_core::segment::SegmentOptions;
use dyndo_core::track::{Track, TrackError};
use futures_util::future::try_join_all;
use image::RgbImage;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::{self, FilterType};
use opendal::Operator;

use crate::decoder::{Decoder, DecoderError, Frame};
use crate::fragment::{Fragment, FragmentError};
use crate::window::{Cell, Window};

/// Quality a sprite is encoded at. A cell is a heavy downscale of its frame, so the
/// detail a higher setting preserves is not there to preserve.
const QUALITY: u8 = 80;

#[derive(Debug, thiserror::Error)]
pub enum SpriteError {
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
    #[error("encoding the sprite failed: {0}")]
    Encode(#[from] image::ImageError),
}

/// The sprite asked for: which frames it shows, and how they are laid out.
///
/// `tile_size` thumbnails go in a row and in a column — a player reads it as the value
/// of the DASH-IF `thumbnail_tile` essential property, where `5` becomes `5x5`, and
/// divides the sprite by it to place a cell. `step` is the milliseconds between one
/// thumbnail and the next, and `time` is what the first thumbnail shows, in
/// milliseconds from the start of the presentation.
///
/// A sprite comes out the size of the track it is cut from, so nothing here states its
/// pixels: a manifest describing one reads the dimensions off that track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sprite {
    pub tile_size: u32,
    pub step: u32,
    pub time: u64,
}

impl Sprite {
    /// Cuts the sprite from `track`.
    ///
    /// # Errors
    ///
    /// Returns a [`SpriteError`] when the track is not video, when it is coded any way
    /// but AVC — the one codec [`Decoder`] reads — when the presentation does not
    /// reach [`time`](Self::time), or when a frame cannot be read, decoded, or encoded.
    pub async fn generate(&self, op: &Operator, track: &Track) -> Result<Bytes, SpriteError> {
        let TrackKind::Video(video) = track.kind() else {
            return Err(SpriteError::NotVideo(track.id().to_string()));
        };
        if !track.codec().starts_with("avc1") {
            return Err(SpriteError::UnsupportedCodec(track.codec().to_string()));
        }

        let window = Window::new(track, self.tile_size * self.tile_size, self.step, self.time)
            .ok_or(SpriteError::NotFound(self.time))?;

        // The only read segment options change is a subtitle document's packaging, and a
        // sprite is only ever cut from a video track — so which options a request asked
        // for delivery in says nothing about how these bytes are read.
        let options = SegmentOptions::default();
        let initialization = track.read_initialization(op, &options).await?;
        // A cell reads the one fragment its frame is in. Reading the whole span they
        // are spread over would be a single request, but it grows with the step while
        // the frames it holds do not: at ten seconds between thumbnails, four bytes in
        // five belong to frames no cell shows.
        //
        // The reads go out together: a sprite is as many of them as it has cells, and
        // waiting for each in turn would pay a round trip per thumbnail.
        let fragments = try_join_all(
            window
                .cells
                .iter()
                .flatten()
                .map(|cell| track.read_range(op, &options, cell.segment.clone())),
        )
        .await?;

        let canvas = Canvas::new(self.tile_size, (video.width, video.height));

        // Decoding a sprite's frames and encoding it is hundreds of milliseconds of CPU,
        // which on the caller's executor would stall every request sharing its thread.
        tokio::task::spawn_blocking(move || canvas.compose(&initialization, &fragments, &window))
            .await
            .expect("composing a sprite does not panic")
    }
}

/// A grid of thumbnails being filled in: a black canvas that decoded frames are scaled
/// into, one cell at a time.
struct Canvas {
    image: RgbImage,
    tile_size: u32,
    cell: (u32, u32),
}

impl Canvas {
    /// An empty canvas the size of the `source` video track, divided into
    /// `tile_size`×`tile_size` cells.
    fn new(tile_size: u32, source: (u32, u32)) -> Self {
        Self {
            image: RgbImage::new(source.0, source.1),
            tile_size,
            cell: cell_size(tile_size, source),
        }
    }

    /// Decodes the frame each cell shows, lays them out, and encodes the sprite.
    /// `fragments` holds the bytes of every cell the presentation reaches, in the order
    /// those cells appear.
    ///
    /// A fragment opens on a keyframe and is decoded on its own, so the cells are
    /// decoded a coreful at a time rather than one after another. They are placed as
    /// each round comes back, which keeps no more frames in memory than there are
    /// threads decoding them.
    ///
    /// A cell the presentation never reaches stays black. DASH-IF expects a trailing
    /// sprite to be partly filled, and a player placing a cell by time never asks for
    /// one of them.
    fn compose(
        mut self,
        initialization: &[u8],
        fragments: &[Bytes],
        window: &Window,
    ) -> Result<Bytes, SpriteError> {
        let cells: Vec<(u32, &Cell, &Bytes)> = window
            .cells
            .iter()
            .enumerate()
            .filter_map(|(index, cell)| cell.as_ref().map(|cell| (index, cell)))
            .zip(fragments)
            .map(|((index, cell), bytes)| {
                let index =
                    u32::try_from(index).expect("a sprite holds no more cells than its tile size");
                (index, cell, bytes)
            })
            .collect();
        let threads = std::thread::available_parallelism().map_or(1, NonZero::get);

        for round in cells.chunks(threads) {
            let frames = std::thread::scope(|scope| {
                let decoding: Vec<_> = round
                    .iter()
                    .map(|(index, cell, bytes)| {
                        scope.spawn(move || -> Result<(u32, Frame), SpriteError> {
                            let mut decoder = Decoder::new(initialization)?;
                            let fragment = Fragment::read(bytes)?;

                            Ok((*index, decoder.frame_at(&fragment, cell.time)?))
                        })
                    })
                    .collect();

                decoding
                    .into_iter()
                    .map(|frame| frame.join().expect("decoding a frame does not panic"))
                    .collect::<Result<Vec<_>, _>>()
            })?;

            for (index, frame) in frames {
                self.place(index, frame)?;
            }
        }

        self.encode()
    }

    /// Scales `frame` into the cell at `index`, counted row by row.
    fn place(&mut self, index: u32, frame: Frame) -> Result<(), SpriteError> {
        let decoded =
            RgbImage::from_raw(frame.width, frame.height, frame.rgb).ok_or(SpriteError::Frame)?;
        let (cell_width, cell_height) = self.cell;

        imageops::overlay(
            &mut self.image,
            &imageops::resize(&decoded, cell_width, cell_height, FilterType::Triangle),
            i64::from(index % self.tile_size * cell_width),
            i64::from(index / self.tile_size * cell_height),
        );

        Ok(())
    }

    /// Encodes the sprite. Cells no frame was placed in stay the black they began as.
    fn encode(self) -> Result<Bytes, SpriteError> {
        let mut encoded = Vec::new();
        JpegEncoder::new_with_quality(&mut encoded, QUALITY).encode_image(&self.image)?;

        Ok(Bytes::from(encoded))
    }
}

/// The pixel size of one thumbnail: the source video track's, divided by the tile each
/// way.
///
/// A tile size the source does not divide evenly leaves the remainder black along the
/// right and bottom edges, rather than shrinking the sprite below the size a manifest
/// read off that track and advertised.
fn cell_size(tile_size: u32, (source_width, source_height): (u32, u32)) -> (u32, u32) {
    (source_width / tile_size, source_height / tile_size)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use image::ImageFormat;

    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");
    const TILE_SIZE: u32 = 5;
    const SOURCE: (u32, u32) = (1920, 1080);

    #[test]
    fn cell_size_divides_the_source_into_a_cell_per_tile() {
        assert_eq!(cell_size(TILE_SIZE, SOURCE), (384, 216));
    }

    /// A tile the source does not divide evenly loses the remainder from every cell,
    /// which the sprite keeps as an unfilled edge.
    #[test]
    fn cell_size_drops_what_the_tile_does_not_divide() {
        assert_eq!(cell_size(7, SOURCE), (274, 154));
    }

    #[test]
    fn a_sprite_is_the_size_of_the_track_it_is_cut_from() {
        assert_eq!(Canvas::new(TILE_SIZE, SOURCE).image.dimensions(), SOURCE);
    }

    /// A sprite is a fixed grid of cells whatever it holds, so one nothing was placed in
    /// still encodes at the size the manifest advertised.
    #[test]
    fn an_empty_sprite_encodes_at_the_advertised_size() {
        let encoded = Canvas::new(TILE_SIZE, SOURCE).encode().unwrap();

        let decoded = image::load_from_memory_with_format(&encoded, ImageFormat::Jpeg).unwrap();
        assert_eq!((decoded.width(), decoded.height()), SOURCE);
    }

    #[test]
    fn place_refuses_a_frame_that_does_not_fill_its_buffer() {
        let error = Canvas::new(TILE_SIZE, SOURCE)
            .place(
                0,
                Frame {
                    width: 16,
                    height: 16,
                    rgb: vec![0; 3],
                },
            )
            .unwrap_err();

        assert!(matches!(error, SpriteError::Frame), "{error}");
    }

    #[test]
    fn place_puts_a_cell_in_the_row_and_column_its_index_names() {
        let (cell_width, cell_height) = cell_size(TILE_SIZE, SOURCE);
        let mut canvas = Canvas::new(TILE_SIZE, SOURCE);

        canvas.place(6, white_frame()).unwrap();

        assert_eq!(
            (
                canvas.image.get_pixel(cell_width, cell_height).0,
                canvas.image.get_pixel(0, 0).0
            ),
            ([255, 255, 255], [0, 0, 0])
        );
    }

    /// The codec a track declares is checked before anything is read, but the
    /// initialization segment has the last word on what the decoder is handed.
    #[test]
    fn compose_refuses_an_initialization_segment_that_is_not_avc() {
        let initialization = fs::read(format!("{FIXTURES}/video_av1_240.mp4")).unwrap();
        let window = Window {
            cells: vec![Some(Cell {
                segment: 0..0,
                time: 0,
            })],
        };

        let error = Canvas::new(TILE_SIZE, SOURCE)
            .compose(&initialization, &[Bytes::new()], &window)
            .unwrap_err();

        assert!(
            matches!(error, SpriteError::Decode(DecoderError::Stream(_))),
            "{error}"
        );
    }

    fn white_frame() -> Frame {
        Frame {
            width: 16,
            height: 16,
            rgb: vec![255; 16 * 16 * 3],
        }
    }
}
