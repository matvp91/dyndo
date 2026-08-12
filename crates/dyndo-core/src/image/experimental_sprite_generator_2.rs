//! Experimental GOP-cadenced JPEG sprite generation using rsmpeg only.

use bytes::Bytes;
use opendal::Operator;
use rsmpeg::avcodec::AVCodecContext;
use rsmpeg::avformat::{AVFormatContextInput, AVIOContextContainer, AVIOContextCustom};
use rsmpeg::avutil::{AVFrame, AVFrameWithImage, AVImage, AVMem};
use rsmpeg::error::RsmpegError;
use rsmpeg::{ffi, swscale::SwsContext};

use super::{FrameExtractorError, frame_extractor::encode_jpeg};
use crate::track::cmaf::ResolvedCmafTrack;

/// A deliberately small proof of concept for GOP-cadenced JPEG sprites.
pub(crate) struct ExperimentalSpriteGenerator2<'a> {
    op: &'a Operator,
    track: &'a ResolvedCmafTrack,
    tile_width: u32,
    tile_height: u32,
    tile_size: u32,
}

impl<'a> ExperimentalSpriteGenerator2<'a> {
    /// Creates a generator for a square `tile_size` by `tile_size` sprite grid.
    pub(crate) fn new(
        op: &'a Operator,
        track: &'a ResolvedCmafTrack,
        tile_width: u32,
        tile_height: u32,
        tile_size: u32,
    ) -> Self {
        Self {
            op,
            track,
            tile_width,
            tile_height,
            tile_size,
        }
    }

    /// Returns sprite `number`, with one tile for each successive video keyframe.
    pub(crate) async fn jpeg(&self, number: u64) -> Result<Bytes, FrameExtractorError> {
        let tiles = u64::from(self.tile_size).pow(2);
        let first = usize::try_from(
            number
                .checked_mul(tiles)
                .ok_or(FrameExtractorError::Extraction)?,
        )
        .map_err(|_| FrameExtractorError::Extraction)?;
        let (Some(first_segment), Some(last_segment)) =
            (self.track.segments().first(), self.track.segments().last())
        else {
            return Err(FrameExtractorError::Extraction);
        };
        let (initialization, media) = tokio::try_join!(
            self.track
                .read_range(self.op, self.track.init_segment().byte_range()),
            self.track.read_range(
                self.op,
                first_segment.byte_range().start..last_segment.byte_range().end,
            ),
        )?;
        let mut input = Vec::with_capacity(initialization.len() + media.len());
        input.extend_from_slice(&initialization);
        input.extend_from_slice(&media);
        let tile_count = usize::try_from(tiles).map_err(|_| FrameExtractorError::Extraction)?;
        let tile_width = self.tile_width;
        let tile_height = self.tile_height;
        let tile_size = self.tile_size;

        tokio::task::spawn_blocking(move || {
            decode_sprite(input, first, tile_count, tile_width, tile_height, tile_size)
        })
        .await
        .map_err(|_| FrameExtractorError::Extraction)?
    }
}

fn decode_sprite(
    input: Vec<u8>,
    mut skipped_keyframes: usize,
    tile_count: usize,
    tile_width: u32,
    tile_height: u32,
    tile_size: u32,
) -> Result<Bytes, FrameExtractorError> {
    let mut position: usize = 0;
    let io = AVIOContextCustom::alloc_context(
        AVMem::new(4_096),
        false,
        Vec::new(),
        Some(Box::new(move |_, output| {
            let end = input.len().min(position.saturating_add(output.len()));
            if position == end {
                return ffi::AVERROR_EOF;
            }
            output[..end - position].copy_from_slice(&input[position..end]);
            let length = end - position;
            position = end;
            i32::try_from(length).unwrap_or(i32::MAX)
        })),
        None,
        None,
    );
    let mut format = AVFormatContextInput::from_io_context(AVIOContextContainer::Custom(io))?;
    let (stream_index, codec) = format
        .find_best_stream(ffi::AVMEDIA_TYPE_VIDEO)?
        .ok_or(FrameExtractorError::Extraction)?;
    let stream = &format.streams()[stream_index];
    let mut decoder = AVCodecContext::new(&codec);
    decoder.apply_codecpar(&stream.codecpar())?;
    decoder.open(None)?;
    let mut sprite = Sprite::new(tile_width, tile_height, tile_size)?;

    while sprite.len() < tile_count {
        let Some(packet) = format.read_packet()? else {
            break;
        };
        if packet.stream_index != stream_index as i32
            || (packet.flags & ffi::AV_PKT_FLAG_KEY as i32) == 0
        {
            continue;
        }
        if skipped_keyframes != 0 {
            skipped_keyframes -= 1;
            continue;
        }
        decoder.send_packet(Some(&packet))?;
        receive(&mut decoder, &mut sprite, tile_count)?;
    }
    decoder.send_packet(None)?;
    receive(&mut decoder, &mut sprite, tile_count)?;
    sprite.encode()
}

fn receive(
    decoder: &mut AVCodecContext,
    sprite: &mut Sprite,
    tile_count: usize,
) -> Result<(), FrameExtractorError> {
    while sprite.len() < tile_count {
        match decoder.receive_frame() {
            Ok(frame) => sprite.push(&frame)?,
            Err(RsmpegError::DecoderDrainError | RsmpegError::DecoderFlushedError) => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

struct Sprite {
    image: AVFrameWithImage,
    tile: AVFrameWithImage,
    tile_width: i32,
    tile_height: i32,
    tile_size: usize,
    scaler: Option<SwsContext>,
    next: usize,
}

impl Sprite {
    fn new(tile_width: u32, tile_height: u32, tile_size: u32) -> Result<Self, FrameExtractorError> {
        let tile_width =
            i32::try_from(tile_width).map_err(|_| FrameExtractorError::InvalidDimensions)?;
        let tile_height =
            i32::try_from(tile_height).map_err(|_| FrameExtractorError::InvalidDimensions)?;
        let tile_size =
            usize::try_from(tile_size).map_err(|_| FrameExtractorError::InvalidDimensions)?;
        let width = tile_width
            .checked_mul(
                i32::try_from(tile_size).map_err(|_| FrameExtractorError::InvalidDimensions)?,
            )
            .filter(|width| *width > 0)
            .ok_or(FrameExtractorError::InvalidDimensions)?;
        let height = tile_height
            .checked_mul(
                i32::try_from(tile_size).map_err(|_| FrameExtractorError::InvalidDimensions)?,
            )
            .filter(|height| *height > 0)
            .ok_or(FrameExtractorError::InvalidDimensions)?;
        let image = AVImage::new(ffi::AV_PIX_FMT_RGB24, width, height, 1)
            .ok_or(FrameExtractorError::Extraction)?;
        let mut image = AVFrameWithImage::new(image);
        clear(&mut image, width, height)?;
        let tile = AVImage::new(ffi::AV_PIX_FMT_RGB24, tile_width, tile_height, 1)
            .ok_or(FrameExtractorError::Extraction)?;

        Ok(Self {
            image,
            tile: AVFrameWithImage::new(tile),
            tile_width,
            tile_height,
            tile_size,
            scaler: None,
            next: 0,
        })
    }

    fn len(&self) -> usize {
        self.next
    }

    fn push(&mut self, frame: &AVFrame) -> Result<(), FrameExtractorError> {
        if self.scaler.is_none() {
            self.scaler = SwsContext::get_context(
                frame.width,
                frame.height,
                frame.format,
                self.tile_width,
                self.tile_height,
                ffi::AV_PIX_FMT_RGB24,
                ffi::SWS_FAST_BILINEAR,
                None,
                None,
                None,
            );
        }
        self.scaler
            .as_mut()
            .ok_or(FrameExtractorError::Extraction)?
            .scale_frame(frame, 0, frame.height, &mut self.tile)?;
        let source = self.tile.image().data()[0];
        let target = self.image.image().data()[0];
        let source_stride = usize::try_from(self.tile.image().linesizes()[0])
            .map_err(|_| FrameExtractorError::Extraction)?;
        let target_stride = usize::try_from(self.image.image().linesizes()[0])
            .map_err(|_| FrameExtractorError::Extraction)?;
        if source.is_null() || target.is_null() {
            return Err(FrameExtractorError::Extraction);
        }
        let width = usize::try_from(self.tile_width)
            .map_err(|_| FrameExtractorError::InvalidDimensions)?
            * 3;
        let height = usize::try_from(self.tile_height)
            .map_err(|_| FrameExtractorError::InvalidDimensions)?;
        let x = self.next % self.tile_size * width;
        let y = self.next / self.tile_size * height;
        for row in 0..height {
            // SAFETY: Both FFmpeg images contain an RGB row at the calculated offset.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    source.add(row * source_stride),
                    target.add((y + row) * target_stride + x),
                    width,
                );
            }
        }
        self.next += 1;
        Ok(())
    }

    fn encode(self) -> Result<Bytes, FrameExtractorError> {
        if self.next == 0 {
            return Err(FrameExtractorError::Extraction);
        }
        encode_jpeg(None, &self.image, self.image.width, self.image.height)
    }
}

fn clear(image: &mut AVFrameWithImage, width: i32, height: i32) -> Result<(), FrameExtractorError> {
    let data = image.image().data()[0];
    let stride = usize::try_from(image.image().linesizes()[0])
        .map_err(|_| FrameExtractorError::Extraction)?;
    if data.is_null() {
        return Err(FrameExtractorError::Extraction);
    }
    let width = usize::try_from(width).map_err(|_| FrameExtractorError::InvalidDimensions)? * 3;
    let height = usize::try_from(height).map_err(|_| FrameExtractorError::InvalidDimensions)?;
    for row in 0..height {
        // SAFETY: FFmpeg allocated one writable RGB row at this offset.
        unsafe { std::ptr::write_bytes(data.add(row * stride), 0, width) };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use image::{GenericImageView, ImageFormat};
    use opendal::{Operator, services::Memory};
    use relative_path::RelativePath;

    use super::ExperimentalSpriteGenerator2;
    use crate::track::ResolvedTrack;

    const FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/two-segment-black-white-h264.mp4");

    #[tokio::test]
    async fn jpeg_generates_a_gop_cadenced_sprite() {
        let op = Operator::new(Memory::default()).unwrap();
        let path = RelativePath::new("video.mp4");
        op.write(path.as_str(), Bytes::from_static(FIXTURE))
            .await
            .unwrap();
        let track = ResolvedTrack::discover(&op, path).await.unwrap();

        let jpeg = ExperimentalSpriteGenerator2::new(&op, track.cmaf().unwrap(), 16, 16, 2)
            .jpeg(0)
            .await
            .unwrap();
        let image = image::load_from_memory_with_format(&jpeg, ImageFormat::Jpeg).unwrap();

        assert_eq!(image.dimensions(), (32, 32));
    }
}
