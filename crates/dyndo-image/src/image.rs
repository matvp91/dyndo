//! The image frames are laid out in, and the JPEG it is encoded to.
//!
//! An image is a fixed grid whatever it holds, so its size follows from the grid,
//! the width one thumbnail is scaled to, and the source track's dimensions — which
//! is what lets a manifest advertise a thumbnail representation without anything
//! being decoded. [`Image::size`] answers that without building one.

use ::image::RgbImage;
use ::image::codecs::jpeg::JpegEncoder;
use ::image::imageops::{self, FilterType};
use bytes::Bytes;

use crate::ThumbnailError;
use crate::avc_decoder::Frame;

/// Quality the image is encoded at. A cell is a heavy downscale of its frame, so
/// the detail a higher setting preserves is not there to preserve.
const QUALITY: u8 = 80;

/// A grid of thumbnails being filled in: a black canvas that decoded frames are
/// scaled into, one cell at a time.
pub(crate) struct Image {
    canvas: RgbImage,
    grid: u32,
    cell: (u32, u32),
}

impl Image {
    /// An empty image holding `grid`×`grid` thumbnails of a `width`×`height` video
    /// track, each thumbnail scaled to `cell_width`.
    pub(crate) fn new(grid: u32, cell_width: u32, source: (u32, u32)) -> Self {
        let (image_width, image_height) = Self::size(grid, cell_width, source);

        Self {
            canvas: RgbImage::new(image_width, image_height),
            grid,
            cell: cell_size(cell_width, source),
        }
    }

    /// The pixel size such an image has, without building one.
    pub(crate) fn size(grid: u32, cell_width: u32, source: (u32, u32)) -> (u32, u32) {
        let (width, height) = cell_size(cell_width, source);

        (width * grid, height * grid)
    }

    /// Scales `frame` into the cell at `index`, counted row by row.
    pub(crate) fn place(&mut self, index: u32, frame: Frame) -> Result<(), ThumbnailError> {
        let decoded = RgbImage::from_raw(frame.width, frame.height, frame.rgb).ok_or(
            ThumbnailError::Container("decoded frame does not fill its buffer"),
        )?;
        let (cell_width, cell_height) = self.cell;

        imageops::overlay(
            &mut self.canvas,
            &imageops::resize(&decoded, cell_width, cell_height, FilterType::Triangle),
            i64::from(index % self.grid * cell_width),
            i64::from(index / self.grid * cell_height),
        );

        Ok(())
    }

    /// Encodes the image. Cells no frame was placed in stay the black they began as.
    pub(crate) fn encode(self) -> Result<Bytes, ThumbnailError> {
        let mut encoded = Vec::new();
        JpegEncoder::new_with_quality(&mut encoded, QUALITY).encode_image(&self.canvas)?;

        Ok(Bytes::from(encoded))
    }
}

/// The pixel size of one thumbnail of a `width`×`height` source scaled to
/// `cell_width`.
///
/// The height is rounded down to an even number: JPEG samples chroma in 2×2 blocks,
/// and an odd height would split a block across the seam between two rows of
/// thumbnails and bleed colour from one into the other.
fn cell_size(cell_width: u32, (width, height): (u32, u32)) -> (u32, u32) {
    let scaled = u64::from(cell_width) * u64::from(height) / u64::from(width);
    let cell_height = u32::try_from(scaled).unwrap_or(u32::MAX) & !1;

    (cell_width, cell_height.max(2))
}

#[cfg(test)]
mod tests {
    use ::image::ImageFormat;

    use super::*;

    const SOURCE: (u32, u32) = (1920, 1080);

    #[test]
    fn cell_size_follows_the_source_aspect() {
        assert_eq!(cell_size(320, SOURCE), (320, 180));
    }

    #[test]
    fn cell_size_rounds_an_odd_height_down() {
        assert_eq!(cell_size(320, (640, 362)), (320, 180));
    }

    #[test]
    fn size_is_the_grid_of_cells() {
        assert_eq!(Image::size(5, 320, SOURCE), (1600, 900));
    }

    #[test]
    fn size_follows_the_grid_it_is_given() {
        assert_eq!(Image::size(4, 320, SOURCE), (1280, 720));
    }

    /// An image is a fixed grid whatever it holds, so one nothing was placed in
    /// still encodes at the size the manifest advertised.
    #[test]
    fn an_empty_image_encodes_at_the_advertised_size() {
        let encoded = Image::new(5, 320, SOURCE).encode().unwrap();

        let decoded = ::image::load_from_memory_with_format(&encoded, ImageFormat::Jpeg).unwrap();
        assert_eq!(
            (decoded.width(), decoded.height()),
            Image::size(5, 320, SOURCE)
        );
    }

    #[test]
    fn place_refuses_a_frame_that_does_not_fill_its_buffer() {
        let error = Image::new(5, 320, SOURCE)
            .place(
                0,
                Frame {
                    width: 16,
                    height: 16,
                    rgb: vec![0; 3],
                },
            )
            .unwrap_err();

        assert!(matches!(error, ThumbnailError::Container(_)), "{error}");
    }

    #[test]
    fn place_puts_a_cell_in_the_row_and_column_its_index_names() {
        let (cell_width, cell_height) = cell_size(320, SOURCE);
        let mut image = Image::new(5, 320, SOURCE);

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
