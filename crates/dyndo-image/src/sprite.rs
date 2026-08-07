//! Sprite sheets cut from a video track.
//!
//! A thumbnail track is not a track dyndo stores: it is a grid of frames cut from a
//! video track at a fixed cadence, addressed by the presentation time its first cell
//! shows. A sheet's duration follows from its cadence as `cells * cadence`, so every
//! one covers the same span and a manifest can describe the whole track with one
//! repeated timeline entry.
//!
//! Two range reads fetch what a sheet needs — the track's initialization segment for
//! the decoder, and the one contiguous range holding every frame its cells show — and
//! the sheet is built entirely in memory.

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

/// A sprite sheet to cut: how its thumbnails are laid out, and how far apart in the
/// presentation they are taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sprite {
    /// Thumbnails per row, and per column.
    ///
    /// A player reads this as the value of the DASH-IF `thumbnail_tile` essential
    /// property — `5` becomes `5x5` — and divides the sheet by it to place a cell.
    pub grid: u32,
    /// The width one thumbnail is scaled to; its height follows the source's aspect.
    pub cell_width: u32,
    /// Milliseconds between one thumbnail and the next.
    pub cadence: u32,
}

impl Sprite {
    /// Thumbnails per sheet.
    pub fn cells(&self) -> u32 {
        self.grid * self.grid
    }

    /// The presentation one sheet covers, in milliseconds.
    pub fn duration(&self) -> u64 {
        u64::from(self.cadence) * u64::from(self.cells())
    }

    /// The pixel size of one sheet cut from a `width`×`height` video track, which is
    /// what a manifest advertises as the thumbnail representation's dimensions.
    pub fn size(&self, source: (u32, u32)) -> (u32, u32) {
        Image::size(self.grid, self.cell_width, source)
    }

    /// Cuts the sheet whose first thumbnail shows `time`, in milliseconds from the
    /// start of the presentation.
    ///
    /// # Errors
    ///
    /// Returns a [`SpriteError`] when the track is not video, when the presentation
    /// does not reach `time`, or when a frame cannot be read, decoded, or encoded.
    pub async fn generate(
        &self,
        op: &Operator,
        track: &Track,
        options: &SegmentOptions,
        time: u64,
    ) -> Result<Bytes, SpriteError> {
        let TrackKind::Video(video) = track.kind() else {
            return Err(SpriteError::NotVideo(track.id()));
        };
        let window = Window::new(track, self.cells(), self.cadence, time)
            .ok_or(SpriteError::NotFound(time))?;

        let initialization = track.read_initialization(op, options).await?;
        let media = track.read_range(op, options, window.range.clone()).await?;

        let codec = track.codec().to_string();
        let image = Image::new(self.grid, self.cell_width, (video.width, video.height));

        // Decoding a sheet's frames and encoding it is hundreds of milliseconds of
        // CPU, which on the caller's executor would stall every request sharing its
        // thread.
        tokio::task::spawn_blocking(move || {
            compose(&codec, &initialization, &media, &window, image)
        })
        .await
        .expect("composing a sheet does not panic")
    }
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
        // A cell the presentation never reaches stays black. DASH-IF expects a
        // trailing sheet to be partly filled, and a player placing a cell by time
        // never asks for one of them.
        let Some(cell) = cell else { continue };
        let bytes = media
            .get(cell.segment.start as usize..cell.segment.end as usize)
            .ok_or(SpriteError::CellOutsideRange)?;
        let fragment = Fragment::read(bytes)?;
        let index = u32::try_from(index).expect("a sheet holds no more cells than its grid");

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

    const SPRITE: Sprite = Sprite {
        grid: 5,
        cell_width: 320,
        cadence: 10_000,
    };

    #[test]
    fn cells_is_the_grid_squared() {
        assert_eq!(SPRITE.cells(), 25);
    }

    #[test]
    fn duration_is_every_cell_at_the_cadence() {
        assert_eq!(SPRITE.duration(), 250_000);
    }

    #[test]
    fn duration_follows_the_grid_it_is_given() {
        assert_eq!(Sprite { grid: 4, ..SPRITE }.duration(), 160_000);
    }

    #[test]
    fn size_is_the_grid_of_cells() {
        assert_eq!(SPRITE.size((1920, 1080)), (1600, 900));
    }

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
        let image = Image::new(SPRITE.grid, SPRITE.cell_width, (1920, 1080));

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
        let image = Image::new(SPRITE.grid, SPRITE.cell_width, (1920, 1080));

        let error = compose("hvc1.1.6.L120.90", &initialization, &[], &window, image).unwrap_err();

        assert!(
            matches!(error, SpriteError::Decode(DecodeError::UnsupportedCodec(_))),
            "{error}"
        );
    }
}
