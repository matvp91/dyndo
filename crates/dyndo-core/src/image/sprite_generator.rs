use std::error::Error;
use std::slice;

use bytes::Bytes;
use opendal::Operator;
use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avformat::{AVFormatContextInput, AVIOContextContainer, AVIOContextCustom};
use rsmpeg::avutil::{AVFrame, AVFrameWithImage, AVImage, AVMem, AVRational};
use rsmpeg::error::RsmpegError;
use rsmpeg::{ffi, swscale::SwsContext};

use crate::track::cmaf::{CmafKind, ResolvedCmafTrack, Segment};

type GenericError = Box<dyn Error + Send + Sync + 'static>;

/// Generates JPEG sprite images from the first frame of regularly spaced CMAF segments.
pub struct SpriteGenerator<'a> {
    op: &'a Operator,
    track: &'a ResolvedCmafTrack,
    tile_width: u32,
    tile_size: u32,
}

#[derive(Clone, Copy)]
struct SpriteLayout {
    tile_width: u32,
    tile_height: u32,
    tile_size: u32,
    width: u32,
    height: u32,
}

struct SpritePlan<'a> {
    layout: SpriteLayout,
    segments: Vec<&'a Segment>,
    targets: Vec<u64>,
}

impl<'a> SpritePlan<'a> {
    fn new(
        track: &'a ResolvedCmafTrack,
        number: u32,
        tile_width: u32,
        tile_size: u32,
    ) -> Result<Self, GenericError> {
        let CmafKind::Video(video) = track.kind() else {
            return Err("sprite source is not video".into());
        };
        let tile_height = u64::from(tile_width)
            .checked_mul(u64::from(video.height))
            .and_then(|height| height.checked_div(u64::from(video.width)))
            .and_then(|height| u32::try_from(height).ok())
            .filter(|height| *height != 0)
            .ok_or("invalid tile dimensions")?;
        let width = tile_width
            .checked_mul(tile_size)
            .filter(|width| *width != 0)
            .ok_or("invalid sprite dimensions")?;
        let height = tile_height
            .checked_mul(tile_size)
            .ok_or("invalid sprite dimensions")?;
        let tile_count = tile_size
            .checked_mul(tile_size)
            .ok_or("invalid sprite dimensions")?;
        let first = usize::try_from(
            number
                .checked_mul(tile_count)
                .ok_or("sprite number is too large")?,
        )?;
        let segments: Vec<_> = track
            .cadence_aligned_segments()
            .skip(first)
            .take(tile_count as usize)
            .collect();
        let targets = segments
            .iter()
            .map(|segment| segment.start_time())
            .collect();

        Ok(Self {
            layout: SpriteLayout {
                tile_width,
                tile_height,
                tile_size,
                width,
                height,
            },
            segments,
            targets,
        })
    }
}

struct FragmentProducer<'a> {
    op: &'a Operator,
    track: &'a ResolvedCmafTrack,
    segments: Vec<&'a Segment>,
}

impl FragmentProducer<'_> {
    async fn stream(self, sender: tokio::sync::mpsc::Sender<Bytes>) -> Result<(), GenericError> {
        self.send(&sender, self.track.init_segment().byte_range())
            .await?;
        for segment in &self.segments {
            self.send(&sender, segment.byte_range()).await?;
        }
        Ok(())
    }

    async fn send(
        &self,
        sender: &tokio::sync::mpsc::Sender<Bytes>,
        range: std::ops::Range<u64>,
    ) -> Result<(), GenericError> {
        let permit = sender
            .reserve()
            .await
            .map_err(|_| "sprite decoder stopped reading")?;
        permit.send(self.track.read_range(self.op, range).await?);
        Ok(())
    }
}

impl<'a> SpriteGenerator<'a> {
    pub fn new(
        op: &'a Operator,
        track: &'a ResolvedCmafTrack,
        tile_width: u32,
        tile_size: u32,
    ) -> Self {
        Self {
            op,
            track,
            tile_width,
            tile_size,
        }
    }

    pub async fn jpeg(&self, number: u32) -> Result<Bytes, GenericError> {
        let plan = SpritePlan::new(self.track, number, self.tile_width, self.tile_size)?;
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        let producer = FragmentProducer {
            op: self.op,
            track: self.track,
            segments: plan.segments,
        };
        let renderer =
            tokio::task::spawn_blocking(move || render(receiver, plan.targets, plan.layout));
        let (producer, renderer) = tokio::join!(producer.stream(sender), renderer);
        producer?;
        renderer?
    }
}

struct FrameDecoder {
    format: AVFormatContextInput,
    decoder: AVCodecContext,
    stream_index: usize,
    time_base: AVRational,
    flushed: bool,
}

impl FrameDecoder {
    fn new(mut chunks: tokio::sync::mpsc::Receiver<Bytes>) -> Result<Self, GenericError> {
        let mut chunk = None;
        let mut position = 0;
        let io = AVIOContextCustom::alloc_context(
            AVMem::new(4_096),
            false,
            Vec::new(),
            Some(Box::new(move |_, buffer| {
                loop {
                    if chunk.is_none() {
                        chunk = chunks.blocking_recv();
                    }
                    let Some(bytes) = &chunk else {
                        return ffi::AVERROR_EOF;
                    };
                    if position == bytes.len() {
                        chunk = None;
                        position = 0;
                        continue;
                    }
                    let length = buffer.len().min(bytes.len() - position);
                    buffer[..length].copy_from_slice(&bytes[position..position + length]);
                    position += length;
                    return i32::try_from(length).unwrap_or(i32::MAX);
                }
            })),
            None,
            None,
        );
        let format = AVFormatContextInput::from_io_context(AVIOContextContainer::Custom(io))?;
        let (stream_index, decoder) = format
            .find_best_stream(ffi::AVMEDIA_TYPE_VIDEO)?
            .ok_or("video stream not found")?;
        let time_base = format.streams()[stream_index].time_base;
        let mut decoder = AVCodecContext::new(&decoder);
        decoder.apply_codecpar(&format.streams()[stream_index].codecpar())?;
        decoder.open(None)?;

        Ok(Self {
            format,
            decoder,
            stream_index,
            time_base,
            flushed: false,
        })
    }

    fn frame_at(&mut self, target: u64) -> Result<Option<AVFrame>, GenericError> {
        loop {
            loop {
                match self.decoder.receive_frame() {
                    Ok(frame)
                        if frame_time(&frame, self.time_base)
                            .is_some_and(|time| time >= target) =>
                    {
                        return Ok(Some(frame));
                    }
                    Ok(_) => {}
                    Err(RsmpegError::DecoderDrainError) => break,
                    Err(RsmpegError::DecoderFlushedError) => return Ok(None),
                    Err(error) => return Err(error.into()),
                }
            }
            if self.flushed {
                return Ok(None);
            }
            let Some(packet) = self.format.read_packet()? else {
                self.decoder.send_packet(None)?;
                self.flushed = true;
                continue;
            };
            if packet.stream_index == self.stream_index as i32
                && packet.flags & ffi::AV_PKT_FLAG_KEY as i32 != 0
            {
                self.decoder.send_packet(Some(&packet))?;
            }
        }
    }
}

struct SpriteEncoder {
    layout: SpriteLayout,
    pixel_format: i32,
    encoder: AVCodecContext,
    sprite: AVFrameWithImage,
    scaler: Option<SwsContext>,
    tile: Option<AVFrameWithImage>,
}

impl SpriteEncoder {
    fn new(layout: SpriteLayout) -> Result<Self, GenericError> {
        let encoder =
            AVCodec::find_encoder(ffi::AV_CODEC_ID_MJPEG).ok_or("JPEG encoder not found")?;
        let pixel_format = encoder
            .pix_fmts()
            .ok_or("JPEG encoder has no pixel formats")?
            .iter()
            .copied()
            .find(|format| {
                *format == ffi::AV_PIX_FMT_YUVJ444P
                    || (*format == ffi::AV_PIX_FMT_YUVJ420P
                        && layout.tile_width.is_multiple_of(2)
                        && layout.tile_height.is_multiple_of(2))
            })
            .ok_or("JPEG encoder has no suitable planar pixel format")?;
        let sprite_width = i32::try_from(layout.width)?;
        let sprite_height = i32::try_from(layout.height)?;
        let sprite_image = AVImage::new(pixel_format, sprite_width, sprite_height, 1)
            .ok_or("could not allocate sprite image")?;
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

    fn add(&mut self, frame: &AVFrame, index: usize) -> Result<(), GenericError> {
        let column = index as u32 % self.layout.tile_size;
        let row = index as u32 / self.layout.tile_size;
        scale_into_sprite(
            frame,
            &mut self.sprite,
            column,
            row,
            self.layout.tile_width,
            self.layout.tile_height,
            self.pixel_format,
            &mut self.scaler,
            &mut self.tile,
        )?;
        Ok(())
    }

    fn jpeg(mut self) -> Result<Bytes, GenericError> {
        self.encoder.send_frame(Some(&self.sprite))?;
        let packet = self.encoder.receive_packet()?;
        let jpeg = unsafe { slice::from_raw_parts(packet.data, packet.size as usize) };
        Ok(Bytes::copy_from_slice(jpeg))
    }
}

fn render(
    receiver: tokio::sync::mpsc::Receiver<Bytes>,
    targets: Vec<u64>,
    layout: SpriteLayout,
) -> Result<Bytes, GenericError> {
    let mut decoder = FrameDecoder::new(receiver)?;
    let mut encoder = SpriteEncoder::new(layout)?;
    for (index, target) in targets.into_iter().enumerate() {
        let Some(frame) = decoder.frame_at(target)? else {
            break;
        };
        encoder.add(&frame, index)?;
    }
    encoder.jpeg()
}

fn frame_time(frame: &AVFrame, time_base: AVRational) -> Option<u64> {
    if frame.best_effort_timestamp == ffi::AV_NOPTS_VALUE || time_base.den <= 0 {
        return None;
    }
    let milliseconds = i128::from(frame.best_effort_timestamp)
        .checked_mul(i128::from(time_base.num))?
        .checked_mul(1_000)?
        / i128::from(time_base.den);
    u64::try_from(milliseconds).ok()
}

#[allow(clippy::too_many_arguments)]
fn scale_into_sprite(
    frame: &AVFrame,
    sprite: &mut AVFrameWithImage,
    column: u32,
    row: u32,
    tile_width: u32,
    tile_height: u32,
    pixel_format: i32,
    scaler: &mut Option<SwsContext>,
    tile: &mut Option<AVFrameWithImage>,
) -> Result<(), GenericError> {
    let scaled_width = i32::try_from(tile_width)?;
    let scaled_height = i32::try_from(tile_height)?;
    if scaler.is_none() {
        *scaler = Some(
            SwsContext::get_context(
                frame.width,
                frame.height,
                frame.format,
                scaled_width,
                scaled_height,
                pixel_format,
                ffi::SWS_FAST_BILINEAR,
                None,
                None,
                None,
            )
            .ok_or("could not create image scaler")?,
        );
    }
    if tile.is_none() {
        let image = AVImage::new(pixel_format, scaled_width, scaled_height, 1)
            .ok_or("could not allocate tile image")?;
        *tile = Some(AVFrameWithImage::new(image));
    }
    let converted = tile.as_mut().ok_or("could not allocate tile image")?;
    scaler
        .as_mut()
        .ok_or("could not create image scaler")?
        .scale_frame(frame, 0, frame.height, converted)?;

    let chroma_shift = usize::from(pixel_format == ffi::AV_PIX_FMT_YUVJ420P);
    for plane in 0..3 {
        let shift = if plane == 0 { 0 } else { chroma_shift };
        let plane_width = usize::try_from(tile_width.div_ceil(1 << shift))?;
        let plane_height = usize::try_from(tile_height.div_ceil(1 << shift))?;
        let x = usize::try_from(column * tile_width)? >> shift;
        let y = usize::try_from(row * tile_height)? >> shift;
        let source_stride = usize::try_from(converted.linesize[plane])?;
        let destination_stride = usize::try_from(sprite.linesize[plane])?;
        for line in 0..plane_height {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    converted.data[plane].add(line * source_stride),
                    sprite.data[plane].add((y + line) * destination_stride + x),
                    plane_width,
                );
            }
        }
    }
    Ok(())
}

fn fill_black(frame: &mut AVFrameWithImage, pixel_format: i32) -> Result<(), GenericError> {
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
        return Err("could not clear sprite image".into());
    }
    Ok(())
}
