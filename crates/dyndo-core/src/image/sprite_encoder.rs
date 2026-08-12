use std::slice;

use bytes::Bytes;
use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avutil::{AVFrame, AVFrameWithImage, AVImage, AVRational};
use rsmpeg::{ffi, swscale::SwsContext};

/// An error encountered while composing or encoding a sprite image.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub(super) struct SpriteEncoderError(String);

impl SpriteEncoderError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl From<rsmpeg::error::RsmpegError> for SpriteEncoderError {
    fn from(error: rsmpeg::error::RsmpegError) -> Self {
        Self::new(format!("FFmpeg failed: {error}"))
    }
}

impl From<std::num::TryFromIntError> for SpriteEncoderError {
    fn from(error: std::num::TryFromIntError) -> Self {
        Self::new(format!("sprite dimensions are out of range: {error}"))
    }
}

#[derive(Clone, Copy)]
pub(super) struct SpriteLayout {
    pub(super) tile_width: u32,
    pub(super) tile_height: u32,
    pub(super) tile_size: u32,
    pub(super) width: u32,
    pub(super) height: u32,
}

pub(super) struct SpriteEncoder {
    layout: SpriteLayout,
    pixel_format: i32,
    encoder: AVCodecContext,
    sprite: AVFrameWithImage,
    scaler: Option<SwsContext>,
    tile: Option<AVFrameWithImage>,
}

impl SpriteEncoder {
    pub(super) fn new(layout: SpriteLayout) -> Result<Self, SpriteEncoderError> {
        let encoder = AVCodec::find_encoder(ffi::AV_CODEC_ID_MJPEG)
            .ok_or_else(|| SpriteEncoderError::new("JPEG encoder not found"))?;
        let pixel_format = encoder
            .pix_fmts()
            .ok_or_else(|| SpriteEncoderError::new("JPEG encoder has no pixel formats"))?
            .iter()
            .copied()
            .find(|format| {
                *format == ffi::AV_PIX_FMT_YUVJ444P
                    || (*format == ffi::AV_PIX_FMT_YUVJ420P
                        && layout.tile_width.is_multiple_of(2)
                        && layout.tile_height.is_multiple_of(2))
            })
            .ok_or_else(|| {
                SpriteEncoderError::new("JPEG encoder has no suitable planar pixel format")
            })?;
        let sprite_width = i32::try_from(layout.width)?;
        let sprite_height = i32::try_from(layout.height)?;
        let sprite_image = AVImage::new(pixel_format, sprite_width, sprite_height, 1)
            .ok_or_else(|| SpriteEncoderError::new("could not allocate sprite image"))?;
        let mut sprite = AVFrameWithImage::new(sprite_image);
        fill_black(&mut sprite, pixel_format)?;
        let mut encoder = AVCodecContext::new(&encoder);
        encoder.set_width(sprite_width);
        encoder.set_height(sprite_height);
        encoder.set_time_base(AVRational { num: 1, den: 1 });
        encoder.set_pix_fmt(pixel_format);
        encoder.open(None)?;

        Ok(Self {
            layout,
            pixel_format,
            encoder,
            sprite,
            scaler: None,
            tile: None,
        })
    }

    pub(super) fn add(&mut self, frame: &AVFrame, index: usize) -> Result<(), SpriteEncoderError> {
        let column = index as u32 % self.layout.tile_size;
        let row = index as u32 / self.layout.tile_size;
        self.scale(frame)?;
        self.copy_tile(column, row)
    }

    fn scale(&mut self, frame: &AVFrame) -> Result<(), SpriteEncoderError> {
        let width = i32::try_from(self.layout.tile_width)?;
        let height = i32::try_from(self.layout.tile_height)?;
        if self.scaler.is_none() {
            self.scaler = Some(
                SwsContext::get_context(
                    frame.width,
                    frame.height,
                    frame.format,
                    width,
                    height,
                    self.pixel_format,
                    ffi::SWS_FAST_BILINEAR,
                    None,
                    None,
                    None,
                )
                .ok_or_else(|| SpriteEncoderError::new("could not create image scaler"))?,
            );
        }
        if self.tile.is_none() {
            let image = AVImage::new(self.pixel_format, width, height, 1)
                .ok_or_else(|| SpriteEncoderError::new("could not allocate tile image"))?;
            self.tile = Some(AVFrameWithImage::new(image));
        }
        self.scaler
            .as_mut()
            .ok_or_else(|| SpriteEncoderError::new("image scaler is unavailable"))?
            .scale_frame(
                frame,
                0,
                frame.height,
                self.tile
                    .as_mut()
                    .ok_or_else(|| SpriteEncoderError::new("tile image is unavailable"))?,
            )?;
        Ok(())
    }

    fn copy_tile(&mut self, column: u32, row: u32) -> Result<(), SpriteEncoderError> {
        let tile = self
            .tile
            .as_ref()
            .ok_or_else(|| SpriteEncoderError::new("tile image is unavailable"))?;
        let chroma_shift = usize::from(self.pixel_format == ffi::AV_PIX_FMT_YUVJ420P);
        for plane in 0..3 {
            let shift = if plane == 0 { 0 } else { chroma_shift };
            let plane_width = usize::try_from(self.layout.tile_width.div_ceil(1 << shift))?;
            let plane_height = usize::try_from(self.layout.tile_height.div_ceil(1 << shift))?;
            let x = usize::try_from(column * self.layout.tile_width)? >> shift;
            let y = usize::try_from(row * self.layout.tile_height)? >> shift;
            let source_stride = usize::try_from(tile.linesize[plane])?;
            let destination_stride = usize::try_from(self.sprite.linesize[plane])?;
            for line in 0..plane_height {
                // SAFETY: both images were allocated for the dimensions and pixel format used
                // above. The calculated tile rectangle is inside the sprite grid.
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        tile.data[plane].add(line * source_stride),
                        self.sprite.data[plane].add((y + line) * destination_stride + x),
                        plane_width,
                    );
                }
            }
        }
        Ok(())
    }

    pub(super) fn jpeg(mut self) -> Result<Bytes, SpriteEncoderError> {
        self.encoder.send_frame(Some(&self.sprite))?;
        let packet = self.encoder.receive_packet()?;
        let jpeg = unsafe { slice::from_raw_parts(packet.data, packet.size as usize) };
        Ok(Bytes::copy_from_slice(jpeg))
    }
}

fn fill_black(frame: &mut AVFrameWithImage, pixel_format: i32) -> Result<(), SpriteEncoderError> {
    let linesizes = frame.linesize.map(|linesize| linesize as isize);
    let result = unsafe {
        ffi::av_image_fill_black(
            frame.data.as_ptr(),
            linesizes.as_ptr(),
            pixel_format,
            ffi::AVCOL_RANGE_JPEG,
            frame.width,
            frame.height,
        )
    };
    if result < 0 {
        return Err(SpriteEncoderError::new("could not clear sprite image"));
    }
    Ok(())
}
