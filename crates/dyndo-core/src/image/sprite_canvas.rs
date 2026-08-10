use bytes::Bytes;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{ImageFormat, RgbImage, imageops};

pub(super) struct SpriteCanvas {
    grid_size: u32,
    tile_width: u32,
    tile_height: u32,
    tiles: u64,
    image: RgbImage,
}

impl SpriteCanvas {
    pub(super) fn new(grid_size: u32, width: u32, height: u32) -> Self {
        Self {
            grid_size,
            tile_width: width / grid_size,
            tile_height: height / grid_size,
            tiles: 0,
            image: RgbImage::new(width, height),
        }
    }

    pub(super) fn tile_dimensions(&self) -> (u32, u32) {
        (self.tile_width, self.tile_height)
    }

    pub(super) fn add(&mut self, jpeg: &[u8]) -> Result<(), image::ImageError> {
        let tile = image::load_from_memory_with_format(jpeg, ImageFormat::Jpeg)?
            .resize_exact(self.tile_width, self.tile_height, FilterType::Triangle)
            .to_rgb8();
        let column = self.tiles % u64::from(self.grid_size);
        let row = self.tiles / u64::from(self.grid_size);
        imageops::replace(
            &mut self.image,
            &tile,
            i64::try_from(column * u64::from(self.tile_width)).unwrap_or(i64::MAX),
            i64::try_from(row * u64::from(self.tile_height)).unwrap_or(i64::MAX),
        );
        self.tiles += 1;

        Ok(())
    }

    pub(super) fn jpeg(&self) -> Result<Bytes, image::ImageError> {
        let mut jpeg = Vec::new();
        JpegEncoder::new(&mut jpeg).encode_image(&self.image)?;
        Ok(Bytes::from(jpeg))
    }
}
