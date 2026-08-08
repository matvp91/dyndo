//! The image frames are laid out in, and the JPEG it is encoded to.
//!
//! An image is a fixed tile of cells whatever it holds: a cell no frame was decoded
//! into stays black rather than shrinking the image, so every sprite cut from an asset
//! comes out the size its manifest advertised.

use ::image::RgbImage;
use ::image::codecs::jpeg::JpegEncoder;
use ::image::imageops::{self, FilterType};
use bytes::Bytes;

use crate::avc_decode::Frame;

/// Quality the image is encoded at. A cell is a heavy downscale of its frame, so the
/// detail a higher setting preserves is not there to preserve.
const QUALITY: u8 = 80;

#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("encoding the sprite failed: {0}")]
    Encode(#[from] ::image::ImageError),
    #[error("decoded frame does not fill its buffer")]
    Frame,
}

/// A tile of thumbnails being filled in: a black canvas that decoded frames are scaled
/// into, one cell at a time.
pub(crate) struct Image {
    canvas: RgbImage,
    tile_size: u32,
    cell: (u32, u32),
}

impl Image {
    /// An empty image of `tile_size`×`tile_size` thumbnails of a `source` video track,
    /// `height` pixels tall give or take what whole cells allow.
    pub(crate) fn new(tile_size: u32, height: u32, source: (u32, u32)) -> Self {
        let (cell_width, cell_height) = cell_size(tile_size, height, source);

        Self {
            canvas: RgbImage::new(cell_width * tile_size, cell_height * tile_size),
            tile_size,
            cell: (cell_width, cell_height),
        }
    }

    /// Scales `frame` into the cell at `index`, counted row by row.
    ///
    /// # Errors
    ///
    /// Returns an [`ImageError`] when the frame's bytes do not match its dimensions.
    pub(crate) fn place(&mut self, index: u32, frame: Frame) -> Result<(), ImageError> {
        let decoded =
            RgbImage::from_raw(frame.width, frame.height, frame.rgb).ok_or(ImageError::Frame)?;
        let (cell_width, cell_height) = self.cell;

        imageops::overlay(
            &mut self.canvas,
            &imageops::resize(&decoded, cell_width, cell_height, FilterType::Triangle),
            i64::from(index % self.tile_size * cell_width),
            i64::from(index / self.tile_size * cell_height),
        );

        Ok(())
    }

    /// Encodes the image. Cells no frame was placed in stay the black they began as.
    ///
    /// # Errors
    ///
    /// Returns an [`ImageError`] when the encoder rejects the image.
    pub(crate) fn encode(self) -> Result<Bytes, ImageError> {
        let mut encoded = Vec::new();
        JpegEncoder::new_with_quality(&mut encoded, QUALITY).encode_image(&self.canvas)?;

        Ok(Bytes::from(encoded))
    }
}

/// The pixel size of one thumbnail: a `tile_size`th of a sprite `height` pixels tall,
/// with its width following the source's aspect.
///
/// Both are rounded down to an even number: JPEG samples chroma in 2×2 blocks, and an
/// odd size would split a block across the seam between two thumbnails and bleed colour
/// from one into the other.
fn cell_size(tile_size: u32, height: u32, (source_width, source_height): (u32, u32)) -> (u32, u32) {
    let cell_height = (height / tile_size) & !1;
    let scaled = u64::from(cell_height) * u64::from(source_width) / u64::from(source_height);
    let cell_width = u32::try_from(scaled).unwrap_or(u32::MAX) & !1;

    (cell_width.max(2), cell_height.max(2))
}

#[cfg(test)]
mod tests {
    use ::image::ImageFormat;

    use super::*;

    const SOURCE: (u32, u32) = (1920, 1080);

    #[test]
    fn cell_size_splits_the_height_across_the_rows_and_follows_the_source_aspect() {
        assert_eq!(cell_size(5, 900, SOURCE), (320, 180));
    }

    #[test]
    fn cell_size_rounds_an_odd_size_down() {
        assert_eq!(cell_size(5, 905, (643, 362)), (318, 180));
    }

    #[test]
    fn an_image_is_the_tile_of_cells_the_height_divides_into() {
        assert_eq!(Image::new(5, 900, SOURCE).canvas.dimensions(), (1600, 900));
    }

    #[test]
    fn an_image_follows_the_tile_size_it_is_given() {
        assert_eq!(Image::new(4, 720, SOURCE).canvas.dimensions(), (1280, 720));
    }

    /// An image is a fixed tile of cells whatever it holds, so one nothing was placed in
    /// still encodes at the size the manifest advertised.
    #[test]
    fn an_empty_image_encodes_at_the_advertised_size() {
        let encoded = Image::new(5, 900, SOURCE).encode().unwrap();

        let decoded = ::image::load_from_memory_with_format(&encoded, ImageFormat::Jpeg).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (1600, 900));
    }

    #[test]
    fn place_refuses_a_frame_that_does_not_fill_its_buffer() {
        let error = Image::new(5, 900, SOURCE)
            .place(
                0,
                Frame {
                    width: 16,
                    height: 16,
                    rgb: vec![0; 3],
                },
            )
            .unwrap_err();

        assert!(matches!(error, ImageError::Frame), "{error}");
    }

    #[test]
    fn place_puts_a_cell_in_the_row_and_column_its_index_names() {
        let (cell_width, cell_height) = cell_size(5, 900, SOURCE);
        let mut image = Image::new(5, 900, SOURCE);

        image.place(6, white_frame()).unwrap();

        assert_eq!(
            (
                image.canvas.get_pixel(cell_width, cell_height).0,
                image.canvas.get_pixel(0, 0).0
            ),
            ([255, 255, 255], [0, 0, 0])
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
