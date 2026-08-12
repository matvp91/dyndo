use std::collections::HashMap;
use std::error::Error;
use std::slice;

use bytes::Bytes;
use image::codecs::jpeg::JpegEncoder;
use image::{RgbImage, imageops};
use opendal::Operator;
use rsmpeg::avcodec::AVCodecContext;
use rsmpeg::avformat::{AVFormatContextInput, AVIOContextContainer, AVIOContextCustom};
use rsmpeg::avutil::{AVFrame, AVFrameWithImage, AVImage, AVMem};
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

    /// Returns the dominant IDR cadence in milliseconds.
    pub fn cadence(track: &ResolvedCmafTrack) -> u64 {
        let ticks = dominant_cadence(track.segments()).unwrap_or(0);
        let timescale = u128::from(track.timescale());
        if timescale == 0 {
            return 0;
        }
        u64::try_from(u128::from(ticks).saturating_mul(1_000) / timescale).unwrap_or(u64::MAX)
    }

    pub async fn jpeg(&self, number: u32) -> Result<Bytes, GenericError> {
        let CmafKind::Video(video) = self.track.kind() else {
            return Err("sprite source is not video".into());
        };
        let tile_height = u64::from(self.tile_width)
            .checked_mul(u64::from(video.height))
            .and_then(|height| height.checked_div(u64::from(video.width)))
            .and_then(|height| u32::try_from(height).ok())
            .filter(|height| *height != 0)
            .ok_or("invalid tile dimensions")?;
        let sprite_width = self
            .tile_width
            .checked_mul(self.tile_size)
            .filter(|width| *width != 0)
            .ok_or("invalid sprite dimensions")?;
        let sprite_height = tile_height
            .checked_mul(self.tile_size)
            .ok_or("invalid sprite dimensions")?;
        let tile_count = self
            .tile_size
            .checked_mul(self.tile_size)
            .ok_or("invalid sprite dimensions")?;
        let first = number
            .checked_mul(tile_count)
            .ok_or("sprite number is too large")? as usize;
        let segments = regular_segments(self.track.segments());
        let segments = segments.iter().skip(first).take(tile_count as usize);
        let initialization = self
            .track
            .read_range(self.op, self.track.init_segment().byte_range())
            .await?;
        let mut sprite = RgbImage::new(sprite_width, sprite_height);

        for (index, segment) in segments.enumerate() {
            let media = self.track.read_range(self.op, segment.byte_range()).await?;
            let initialization = initialization.clone();
            let tile_width = self.tile_width;
            let tile = tokio::task::spawn_blocking(move || {
                decode_first_frame(initialization, media, tile_width, tile_height)
            })
            .await??;
            let column = index as u32 % self.tile_size;
            let row = index as u32 / self.tile_size;
            imageops::replace(
                &mut sprite,
                &tile,
                i64::from(column * self.tile_width),
                i64::from(row * tile_height),
            );
        }

        let mut jpeg = Vec::new();
        JpegEncoder::new(&mut jpeg).encode_image(&sprite)?;
        Ok(Bytes::from(jpeg))
    }
}

fn regular_segments(segments: &[Segment]) -> Vec<&Segment> {
    let Some(anchor) = segments.first().map(Segment::unscaled_start_time) else {
        return Vec::new();
    };
    let Some(cadence) = dominant_cadence(segments) else {
        return Vec::new();
    };
    let tolerance = cadence.div_ceil(100);

    segments
        .iter()
        .filter(|segment| {
            let offset = segment.unscaled_start_time().saturating_sub(anchor);
            let remainder = offset % cadence;
            remainder <= tolerance || cadence - remainder <= tolerance
        })
        .collect()
}

fn dominant_cadence(segments: &[Segment]) -> Option<u64> {
    let mut durations = HashMap::new();
    for segment in segments {
        let duration = segment
            .unscaled_end_time()
            .saturating_sub(segment.unscaled_start_time());
        *durations.entry(duration).or_insert(0_usize) += 1;
    }
    durations
        .into_iter()
        .max_by_key(|&(duration, occurrences)| (occurrences, duration))
        .map(|(duration, _)| duration)
        .filter(|duration| *duration != 0)
}

fn decode_first_frame(
    initialization: Bytes,
    media: Bytes,
    width: u32,
    height: u32,
) -> Result<RgbImage, GenericError> {
    let mut chunks = [initialization, media].into_iter();
    let mut chunk = chunks.next();
    let mut position = 0;
    let io = AVIOContextCustom::alloc_context(
        AVMem::new(4_096),
        false,
        Vec::new(),
        Some(Box::new(move |_, buffer| {
            loop {
                let Some(bytes) = &chunk else {
                    return ffi::AVERROR_EOF;
                };
                if position == bytes.len() {
                    chunk = chunks.next();
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
    let mut format = AVFormatContextInput::from_io_context(AVIOContextContainer::Custom(io))?;
    let (stream_index, decoder) = format
        .find_best_stream(ffi::AVMEDIA_TYPE_VIDEO)?
        .ok_or("video stream not found")?;
    let mut decoder = AVCodecContext::new(&decoder);
    decoder.apply_codecpar(&format.streams()[stream_index].codecpar())?;
    decoder.open(None)?;

    let frame = first_frame(&mut format, &mut decoder, stream_index)?;
    scale_to_rgb(&frame, width, height)
}

fn first_frame(
    format: &mut AVFormatContextInput,
    decoder: &mut AVCodecContext,
    stream_index: usize,
) -> Result<AVFrame, GenericError> {
    while let Some(packet) = format.read_packet()? {
        if packet.stream_index != stream_index as i32 {
            continue;
        }
        decoder.send_packet(Some(&packet))?;
        match decoder.receive_frame() {
            Ok(frame) => return Ok(frame),
            Err(RsmpegError::DecoderDrainError) => {}
            Err(error) => return Err(error.into()),
        }
    }
    decoder.send_packet(None)?;
    Ok(decoder.receive_frame()?)
}

fn scale_to_rgb(frame: &AVFrame, width: u32, height: u32) -> Result<RgbImage, GenericError> {
    let scaled_width = i32::try_from(width)?;
    let scaled_height = i32::try_from(height)?;
    let mut scaler = SwsContext::get_context(
        frame.width,
        frame.height,
        frame.format,
        scaled_width,
        scaled_height,
        ffi::AV_PIX_FMT_RGB24,
        ffi::SWS_FAST_BILINEAR,
        None,
        None,
        None,
    )
    .ok_or("could not create image scaler")?;
    let image = AVImage::new(ffi::AV_PIX_FMT_RGB24, scaled_width, scaled_height, 1)
        .ok_or("could not allocate image")?;
    let mut converted = AVFrameWithImage::new(image);
    scaler.scale_frame(frame, 0, frame.height, &mut converted)?;

    let row_bytes = usize::try_from(scaled_width)? * 3;
    let stride = usize::try_from(converted.linesize[0])?;
    let rows = usize::try_from(scaled_height)?;
    let source = unsafe { slice::from_raw_parts(converted.data[0], stride * rows) };
    let mut pixels = Vec::with_capacity(row_bytes * rows);
    for row in source.chunks_exact(stride).take(rows) {
        pixels.extend_from_slice(&row[..row_bytes]);
    }
    RgbImage::from_raw(width, height, pixels).ok_or_else(|| "invalid RGB image".into())
}
