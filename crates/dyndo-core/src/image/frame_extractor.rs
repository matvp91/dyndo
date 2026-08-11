use std::slice;

use bytes::Bytes;
use opendal::Operator;
use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avformat::{AVFormatContextInput, AVIOContextContainer, AVIOContextCustom};
use rsmpeg::avutil::{AVFrame, AVFrameWithImage, AVImage, AVMem, AVRational};
use rsmpeg::error::RsmpegError;
use rsmpeg::{ffi, swscale::SwsContext};

use crate::segment::Segment;
use crate::track::cmaf::CmafTrack;

/// An error encountered while extracting a video frame.
#[derive(Debug, thiserror::Error)]
pub enum FrameExtractorError {
    #[error("time {0} ms is outside the video track")]
    TimeOutsideTrack(u64),
    #[error("invalid JPEG frame dimensions")]
    InvalidDimensions,
    #[error(transparent)]
    Storage(#[from] opendal::Error),
    #[error(transparent)]
    Ffmpeg(#[from] RsmpegError),
    #[error("could not extract a JPEG frame")]
    Extraction,
}

/// Extracts JPEG frames from a video track.
pub struct FrameExtractor<'a> {
    op: &'a Operator,
    track: &'a CmafTrack,
}

impl<'a> FrameExtractor<'a> {
    /// Creates a frame extractor for `track`.
    pub fn new(op: &'a Operator, track: &'a CmafTrack) -> Self {
        Self { op, track }
    }

    /// Returns the displayed frame at `time`, expressed in milliseconds, as a JPEG scaled to
    /// `width` by `height`.
    ///
    /// Only the initialization data and the media segment containing `time` are
    /// read from storage. Decoding and JPEG encoding run on Tokio's blocking
    /// pool, so callers can extract multiple frames without occupying async
    /// executor workers.
    ///
    /// # Errors
    ///
    /// Returns an error when `time` falls outside the track, the requested
    /// dimensions are unsupported, storage cannot be read, or FFmpeg cannot
    /// decode or encode the frame.
    pub async fn jpeg(
        &self,
        time: u64,
        width: u32,
        height: u32,
    ) -> Result<Bytes, FrameExtractorError> {
        let width = i32::try_from(width).map_err(|_| FrameExtractorError::InvalidDimensions)?;
        let height = i32::try_from(height).map_err(|_| FrameExtractorError::InvalidDimensions)?;
        if width <= 0 || height <= 0 {
            return Err(FrameExtractorError::InvalidDimensions);
        }
        let segment = self
            .track
            .segments()
            .iter()
            .find(|segment| segment.start_time() <= time && time < segment.end_time())
            .ok_or(FrameExtractorError::TimeOutsideTrack(time))?;
        let input = self.read_segment(segment).await?;

        tokio::task::spawn_blocking(move || encode_frame_as_jpeg(input, time, width, height))
            .await
            .map_err(|_| FrameExtractorError::Extraction)?
    }

    async fn read_segment(&self, segment: &Segment) -> Result<Vec<u8>, FrameExtractorError> {
        let path = self.track.path().as_str();
        let (initialization, media) = tokio::try_join!(
            self.op
                .read_with(path)
                .range(self.track.init_segment().byte_range()),
            self.op.read_with(path).range(segment.byte_range()),
        )?;
        let initialization = initialization.to_bytes();
        let media = media.to_bytes();
        let mut input = Vec::with_capacity(initialization.len() + media.len());
        input.extend_from_slice(&initialization);
        input.extend_from_slice(&media);

        Ok(input)
    }
}

fn encode_frame_as_jpeg(
    input: Vec<u8>,
    time: u64,
    width: i32,
    height: i32,
) -> Result<Bytes, FrameExtractorError> {
    let mut position: usize = 0;
    let io = AVIOContextCustom::alloc_context(
        AVMem::new(4_096),
        false,
        Vec::new(),
        Some(Box::new(move |_, buffer| {
            let end = input.len().min(position.saturating_add(buffer.len()));
            if position == end {
                return ffi::AVERROR_EOF;
            }
            let length = end - position;
            buffer[..length].copy_from_slice(&input[position..end]);
            position = end;
            i32::try_from(length).unwrap_or(i32::MAX)
        })),
        None,
        None,
    );
    let mut format = AVFormatContextInput::from_io_context(AVIOContextContainer::Custom(io))?;
    let (stream_index, decoder) = format
        .find_best_stream(ffi::AVMEDIA_TYPE_VIDEO)?
        .ok_or(FrameExtractorError::Extraction)?;
    let stream = &format.streams()[stream_index];
    let time_base = stream.time_base;
    let mut decoder_context = AVCodecContext::new(&decoder);
    decoder_context.apply_codecpar(&stream.codecpar())?;
    decoder_context.open(None)?;

    let frame = decode_frame(
        &mut format,
        &mut decoder_context,
        stream_index,
        time_base,
        time,
    )?;
    encode_jpeg(&decoder_context, &frame, width, height)
}

fn decode_frame(
    format: &mut AVFormatContextInput,
    decoder: &mut AVCodecContext,
    stream_index: usize,
    time_base: AVRational,
    time: u64,
) -> Result<AVFrame, FrameExtractorError> {
    let mut candidate = None;
    while let Some(packet) = format.read_packet()? {
        if packet.stream_index != stream_index as i32 {
            continue;
        }
        decoder.send_packet(Some(&packet))?;
        loop {
            match decoder.receive_frame() {
                Ok(frame) if frame_time(&frame, time_base).is_some_and(|pts| pts > time) => {
                    return Ok(candidate.unwrap_or(frame));
                }
                Ok(frame) => candidate = Some(frame),
                Err(RsmpegError::DecoderDrainError) => break,
                Err(error) => return Err(error.into()),
            }
        }
    }

    decoder.send_packet(None)?;
    loop {
        match decoder.receive_frame() {
            Ok(frame) if frame_time(&frame, time_base).is_some_and(|pts| pts > time) => {
                return Ok(candidate.unwrap_or(frame));
            }
            Ok(frame) => candidate = Some(frame),
            Err(RsmpegError::DecoderFlushedError | RsmpegError::DecoderDrainError) => break,
            Err(error) => return Err(error.into()),
        }
    }
    candidate.ok_or(FrameExtractorError::Extraction)
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

fn encode_jpeg(
    decoder: &AVCodecContext,
    frame: &AVFrame,
    width: i32,
    height: i32,
) -> Result<Bytes, FrameExtractorError> {
    let encoder =
        AVCodec::find_encoder(ffi::AV_CODEC_ID_MJPEG).ok_or(FrameExtractorError::Extraction)?;
    let pixel_format = encoder
        .pix_fmts()
        .and_then(|formats| formats.first())
        .copied()
        .ok_or(FrameExtractorError::Extraction)?;
    let mut encoder_context = AVCodecContext::new(&encoder);
    encoder_context.set_bit_rate(decoder.bit_rate);
    encoder_context.set_width(width);
    encoder_context.set_height(height);
    encoder_context.set_time_base(AVRational { num: 1, den: 1 });
    encoder_context.set_pix_fmt(pixel_format);
    encoder_context.open(None)?;

    let mut scaler = SwsContext::get_context(
        frame.width,
        frame.height,
        frame.format,
        width,
        height,
        pixel_format,
        ffi::SWS_FAST_BILINEAR,
        None,
        None,
        None,
    )
    .ok_or(FrameExtractorError::Extraction)?;
    let image =
        AVImage::new(pixel_format, width, height, 1).ok_or(FrameExtractorError::Extraction)?;
    let mut converted = AVFrameWithImage::new(image);
    scaler.scale_frame(frame, 0, frame.height, &mut converted)?;

    encoder_context.send_frame(Some(&converted))?;
    let packet = encoder_context.receive_packet()?;
    // SAFETY: FFmpeg owns `packet.data`, and guarantees that it points to
    // `packet.size` initialized bytes for the lifetime of `packet`.
    let jpeg = unsafe { slice::from_raw_parts(packet.data, packet.size as usize) };
    Ok(Bytes::copy_from_slice(jpeg))
}
