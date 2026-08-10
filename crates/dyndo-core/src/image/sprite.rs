use bytes::Bytes;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{ImageFormat, RgbImage, imageops};

/// An error encountered while building a sprite image.
#[derive(Debug, thiserror::Error)]
pub enum SpriteError {
    #[error("tile size must be greater than zero")]
    InvalidTileSize,
    #[error("sprite dimensions {width}x{height} must be divisible by tile size {tile_size}")]
    InvalidDimensions {
        tile_size: u32,
        width: u32,
        height: u32,
    },
    #[error("the sprite already contains its maximum of {0} tiles")]
    Full(u64),
    #[error(transparent)]
    Image(#[from] image::ImageError),
}

/// Builds a square grid of JPEG images in row-major order.
pub struct Sprite {
    tile_size: u32,
    tile_width: u32,
    tile_height: u32,
    tiles: u64,
    image: RgbImage,
}

impl Sprite {
    /// Creates a `tile_size` by `tile_size` grid with the given overall dimensions.
    ///
    /// # Errors
    ///
    /// Returns an error if `tile_size` is zero or the sprite dimensions cannot
    /// be divided evenly into the requested grid.
    pub fn new(tile_size: u32, width: u32, height: u32) -> Result<Self, SpriteError> {
        if tile_size == 0 {
            return Err(SpriteError::InvalidTileSize);
        }
        if width == 0
            || height == 0
            || !width.is_multiple_of(tile_size)
            || !height.is_multiple_of(tile_size)
        {
            return Err(SpriteError::InvalidDimensions {
                tile_size,
                width,
                height,
            });
        }

        Ok(Self {
            tile_size,
            tile_width: width / tile_size,
            tile_height: height / tile_size,
            tiles: 0,
            image: RgbImage::new(width, height),
        })
    }

    /// Adds a JPEG to the next available tile, from left to right and top to bottom.
    ///
    /// The JPEG is resized to fill its tile.
    ///
    /// # Errors
    ///
    /// Returns an error if the sprite is full or `jpeg` cannot be decoded.
    pub fn add(&mut self, jpeg: &[u8]) -> Result<(), SpriteError> {
        let capacity = self.capacity();
        if self.tiles == capacity {
            return Err(SpriteError::Full(capacity));
        }

        let tile = image::load_from_memory_with_format(jpeg, ImageFormat::Jpeg)?
            .resize_exact(self.tile_width, self.tile_height, FilterType::Triangle)
            .to_rgb8();
        let column = self.tiles % u64::from(self.tile_size);
        let row = self.tiles / u64::from(self.tile_size);
        imageops::replace(
            &mut self.image,
            &tile,
            i64::try_from(column * u64::from(self.tile_width)).unwrap_or(i64::MAX),
            i64::try_from(row * u64::from(self.tile_height)).unwrap_or(i64::MAX),
        );
        self.tiles += 1;

        Ok(())
    }

    /// Encodes the current sprite as JPEG bytes.
    ///
    /// Unfilled tiles remain black.
    ///
    /// # Errors
    ///
    /// Returns an error if the sprite cannot be JPEG-encoded.
    pub fn jpeg(&self) -> Result<Bytes, SpriteError> {
        let mut jpeg = Vec::new();
        JpegEncoder::new(&mut jpeg).encode_image(&self.image)?;
        Ok(Bytes::from(jpeg))
    }

    /// Returns the number of JPEGs currently in the sprite.
    pub fn len(&self) -> u64 {
        self.tiles
    }

    /// Returns whether the sprite contains no JPEGs.
    pub fn is_empty(&self) -> bool {
        self.tiles == 0
    }

    /// Returns the maximum number of JPEGs the sprite can contain.
    pub fn capacity(&self) -> u64 {
        u64::from(self.tile_size).pow(2)
    }
}

#[cfg(test)]
mod tests {
    use image::{ImageFormat, Rgb, RgbImage};

    use super::{Sprite, SpriteError};

    fn jpeg() -> Vec<u8> {
        let image = RgbImage::from_pixel(2, 2, Rgb([255, 0, 0]));
        let mut jpeg = Vec::new();
        JpegEncoder::new(&mut jpeg).encode_image(&image).unwrap();
        jpeg
    }

    use image::codecs::jpeg::JpegEncoder;

    #[test]
    fn new_should_reject_zero_tile_size() {
        assert!(matches!(
            Sprite::new(0, 100, 100),
            Err(SpriteError::InvalidTileSize)
        ));
    }

    #[test]
    fn new_should_reject_dimensions_not_divisible_by_tile_size() {
        assert!(matches!(
            Sprite::new(3, 100, 90),
            Err(SpriteError::InvalidDimensions { .. })
        ));
    }

    #[test]
    fn add_should_reject_tile_beyond_capacity() {
        let mut sprite = Sprite::new(1, 2, 2).unwrap();
        sprite.add(&jpeg()).unwrap();

        assert!(matches!(sprite.add(&jpeg()), Err(SpriteError::Full(1))));
    }

    #[test]
    fn jpeg_should_preserve_sprite_dimensions() {
        let mut sprite = Sprite::new(2, 8, 4).unwrap();
        sprite.add(&jpeg()).unwrap();
        let jpeg = sprite.jpeg().unwrap();
        let image = image::load_from_memory_with_format(&jpeg, ImageFormat::Jpeg).unwrap();

        assert_eq!((image.width(), image.height()), (8, 4));
    }
}
