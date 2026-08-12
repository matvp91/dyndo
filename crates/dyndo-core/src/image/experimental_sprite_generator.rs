//! Experimental, single-pass JPEG sprite generation.

use std::collections::VecDeque;
use std::ops::Range;
use std::slice;
use std::sync::{Arc, Mutex};
use std::sync::OnceLock;

use bytes::Bytes;
use futures_util::StreamExt;
use image::{RgbImage, codecs::jpeg::JpegEncoder};
use opendal::Operator;
use rsmpeg::avcodec::{AVCodecContext, AVPacket};
use rsmpeg::avformat::{AVFormatContextInput, AVIOContextContainer, AVIOContextCustom};
use rsmpeg::avutil::{AVFrame, AVFrameWithImage, AVImage, AVMem, AVRational};
use rsmpeg::error::RsmpegError;
use rsmpeg::{ffi, swscale::SwsContext};
use tokio::sync::{Semaphore, mpsc};

use crate::track::cmaf::{CmafReadError, ResolvedCmafTrack};

const MAX_CONCURRENT_SPRITES: usize = 2;
static SPRITE_GENERATIONS: OnceLock<Semaphore> = OnceLock::new();

/// Selects the source frame for a sprite tile.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameSelection {
    /// Return the displayed frame at the requested time.
    #[default]
    Exact,
    /// Return the preceding random-access frame for each requested cadence time.
    PreviousKeyframe,
}

/// A deliberately small proof of concept for generating one JPEG sprite.
pub(crate) struct ExperimentalSpriteGenerator<'a> {
    op: &'a Operator,
    track: &'a ResolvedCmafTrack,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SpriteGeneratorError {
    #[error(transparent)]
    Cmaf(#[from] CmafReadError),
    #[error(transparent)]
    Ffmpeg(#[from] RsmpegError),
    #[error("media stream failed: {0}")]
    Stream(String),
    #[error("could not generate sprite: {0}")]
    Generation(&'static str),
}

impl<'a> ExperimentalSpriteGenerator<'a> {
    pub(crate) fn new(op: &'a Operator, track: &'a ResolvedCmafTrack) -> Self {
        Self { op, track }
    }

    /// Plans the contiguous CMAF media range required for `times`.
    ///
    /// The eventual decoder consumes this range once, after the initialization segment.
    pub(crate) fn media_range(&self, times: &[u64]) -> Option<Range<u64>> {
        let first = *times.first()?;
        let last = *times.last()?;
        let segments = self.track.segments();
        let first = segments
            .iter()
            .find(|segment| segment.start_time() <= first && first < segment.end_time())?;
        let last = segments
            .iter()
            .find(|segment| segment.start_time() <= last && last < segment.end_time())?;
        Some(first.byte_range().start..last.byte_range().end)
    }

    /// Validates the requested cadence timestamps.
    pub(crate) fn source_times(
        &self,
        times: &[u64],
        selection: FrameSelection,
    ) -> Option<Vec<u64>> {
        if selection != FrameSelection::PreviousKeyframe {
            return None;
        }
        times
            .iter()
            .map(|&time| {
                self.track
                    .segments()
                    .iter()
                    .any(|segment| segment.start_time() <= time && time < segment.end_time())
                    .then_some(time)
            })
            .collect()
    }

    /// Streams one contiguous media window to the synchronous FFmpeg reader.
    pub(crate) async fn input(&self, times: &[u64]) -> Result<SpriteInput, SpriteGeneratorError> {
        let media_range = self.media_range(times).ok_or(CmafReadError::Range)?;
        let initialization = self
            .track
            .read_range(self.op, self.track.init_segment().byte_range())
            .await?;
        let (sender, media) = mpsc::channel::<Result<Bytes, String>>(2);

        if let Some(path) = self.track.source_path().map(ToOwned::to_owned) {
            let op = self.op.clone();
            tokio::spawn(async move {
                let result = async {
                    let mut stream = op
                        .reader(path.as_str())
                        .await
                        .map_err(CmafReadError::from)?
                        .into_bytes_stream(media_range)
                        .await
                        .map_err(CmafReadError::from)?;
                    while let Some(chunk) = stream.next().await {
                        sender
                            .send(Ok(chunk.map_err(|error| {
                                SpriteGeneratorError::Stream(error.to_string())
                            })?))
                            .await
                            .map_err(|_| SpriteGeneratorError::Generation("sprite decoder stopped"))?;
                    }
                    Ok::<_, SpriteGeneratorError>(())
                }
                .await;
                if let Err(error) = result {
                    let _ = sender.send(Err(error.to_string())).await;
                }
            });
        } else {
            let media = self.track.read_range(self.op, media_range).await?;
            sender
                .send(Ok(media))
                .await
                .map_err(|_| SpriteGeneratorError::Generation("sprite decoder stopped"))?;
        }

        Ok(SpriteInput {
            initialization,
            media,
        })
    }

    pub(crate) async fn jpeg(
        &self,
        times: &[u64],
        tile_size: u32,
        tile_width: u32,
        tile_height: u32,
        selection: FrameSelection,
    ) -> Result<Bytes, SpriteGeneratorError> {
        let _permit = SPRITE_GENERATIONS
            .get_or_init(|| Semaphore::new(MAX_CONCURRENT_SPRITES))
            .acquire()
            .await
            .map_err(|_| SpriteGeneratorError::Generation("sprite semaphore closed"))?;
        let targets = self
            .source_times(times, selection)
            .ok_or(SpriteGeneratorError::Generation("invalid frame times"))?;
        let input = self.input(times).await?;
        let width = tile_size
            .checked_mul(tile_width)
            .ok_or(SpriteGeneratorError::Generation("sprite width overflow"))?;
        let height = tile_size
            .checked_mul(tile_height)
            .ok_or(SpriteGeneratorError::Generation("sprite height overflow"))?;
        tokio::task::spawn_blocking(move || {
            decode_sprite(
                input,
                targets,
                tile_size,
                tile_width,
                tile_height,
                width,
                height,
            )
        })
        .await
        .map_err(|_| SpriteGeneratorError::Generation("decoder task cancelled"))?
    }
}

/// The bounded CMAF input a single sprite decoder receives.
pub(crate) struct SpriteInput {
    pub(crate) initialization: Bytes,
    pub(crate) media: mpsc::Receiver<Result<Bytes, String>>,
}

fn decode_sprite(
    input: SpriteInput,
    targets: Vec<u64>,
    tile_size: u32,
    tile_width: u32,
    tile_height: u32,
    width: u32,
    height: u32,
) -> Result<Bytes, SpriteGeneratorError> {
    let stream_error = Arc::new(Mutex::new(None));
    let callback_error = Arc::clone(&stream_error);
    let mut initialization = input.initialization;
    let mut initialization_position = 0;
    let mut media = input.media;
    let mut chunk = Bytes::new();
    let mut chunk_position = 0;
    let io = AVIOContextCustom::alloc_context(
        AVMem::new(4_096),
        false,
        Vec::new(),
        Some(Box::new(move |_, output| {
            let mut written = 0;
            while written < output.len() {
                let (source, position) = if initialization_position < initialization.len() {
                    (&initialization, &mut initialization_position)
                } else {
                    while chunk_position == chunk.len() {
                        match media.blocking_recv() {
                            Some(Ok(next)) => {
                                chunk = next;
                                chunk_position = 0;
                            }
                            Some(Err(error)) => {
                                *callback_error.lock().unwrap() = Some(error);
                                return ffi::AVERROR_EXTERNAL;
                            }
                            None => {
                                return if written == 0 {
                                    ffi::AVERROR_EOF
                                } else {
                                    i32::try_from(written).unwrap_or(i32::MAX)
                                };
                            }
                        }
                    }
                    (&chunk, &mut chunk_position)
                };
                let length = (output.len() - written).min(source.len() - *position);
                output[written..written + length]
                    .copy_from_slice(&source[*position..*position + length]);
                *position += length;
                written += length;
            }
            i32::try_from(written).unwrap_or(i32::MAX)
        })),
        None,
        None,
    );
    let result = decode_sprite_input(
        io,
        targets,
        tile_size,
        tile_width,
        tile_height,
        width,
        height,
    );
    if let Some(error) = stream_error.lock().unwrap().take() {
        return Err(SpriteGeneratorError::Stream(error));
    }
    result
}

fn decode_sprite_input(
    io: AVIOContextCustom,
    targets: Vec<u64>,
    tile_size: u32,
    tile_width: u32,
    tile_height: u32,
    width: u32,
    height: u32,
) -> Result<Bytes, SpriteGeneratorError> {
    let mut format = AVFormatContextInput::from_io_context(AVIOContextContainer::Custom(io))?;
    let (stream_index, decoder) = format
        .find_best_stream(ffi::AVMEDIA_TYPE_VIDEO)?
        .ok_or(SpriteGeneratorError::Generation("video stream not found"))?;
    let stream = &format.streams()[stream_index];
    let mut context = AVCodecContext::new(&decoder);
    context.apply_codecpar(&stream.codecpar())?;
    context.open(None)?;
    let mut cadence = Cadence::new(targets, stream.time_base);
    let mut canvas = Canvas::new(width, height, tile_size, tile_width, tile_height);
    let mut copies = VecDeque::new();

    while !cadence.is_complete() {
        let Some(packet) = format.read_packet()? else {
            break;
        };
        if packet.stream_index != stream_index as i32
            || (packet.flags & ffi::AV_PKT_FLAG_KEY as i32) == 0
        {
            continue;
        }
        if let Some((packet, count)) = cadence.push(packet) {
            copies.push_back(count);
            context.send_packet(Some(&packet))?;
            receive(&mut context, &mut canvas, &mut copies)?;
        }
    }

    if let Some((packet, count)) = cadence.finish() {
        copies.push_back(count);
        context.send_packet(Some(&packet))?;
        receive(&mut context, &mut canvas, &mut copies)?;
    }
    if !copies.is_empty() {
        context.send_packet(None)?;
        receive(&mut context, &mut canvas, &mut copies)?;
    }
    if !copies.is_empty() {
        return Err(SpriteGeneratorError::Generation("not enough decoded iframes"));
    }
    canvas.finish()
}

fn receive(
    decoder: &mut AVCodecContext,
    canvas: &mut Canvas,
    copies: &mut VecDeque<usize>,
) -> Result<(), SpriteGeneratorError> {
    loop {
        match decoder.receive_frame() {
            Ok(frame) => canvas.push(frame, copies.pop_front().unwrap_or_default())?,
            Err(RsmpegError::DecoderDrainError | RsmpegError::DecoderFlushedError) => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
}

struct Cadence {
    targets: Vec<u64>,
    next: usize,
    candidate: Option<AVPacket>,
    offset: Option<i128>,
    time_base: AVRational,
}

impl Cadence {
    fn new(targets: Vec<u64>, time_base: AVRational) -> Self {
        Self {
            targets,
            next: 0,
            candidate: None,
            offset: None,
            time_base,
        }
    }

    fn is_complete(&self) -> bool {
        self.next == self.targets.len()
    }

    fn push(&mut self, packet: AVPacket) -> Option<(AVPacket, usize)> {
        let time = packet_time(&packet, self.time_base)?;
        let first = *self.targets.first()?;
        let offset = self
            .offset
            .get_or_insert(i128::from(time) - i128::from(first));
        let time = u64::try_from(i128::from(time) - *offset).ok()?;
        let candidate = self.candidate.replace(packet)?;
        let start = self.next;
        self.next += self.targets[self.next..].partition_point(|target| *target < time);
        (self.next > start).then_some((candidate, self.next - start))
    }

    fn finish(&mut self) -> Option<(AVPacket, usize)> {
        let candidate = self.candidate.take()?;
        let count = self.targets.len().checked_sub(self.next)?;
        self.next = self.targets.len();
        (count != 0).then_some((candidate, count))
    }
}

fn packet_time(packet: &AVPacket, time_base: AVRational) -> Option<u64> {
    let timestamp = (packet.pts != ffi::AV_NOPTS_VALUE)
        .then_some(packet.pts)
        .or((packet.dts != ffi::AV_NOPTS_VALUE).then_some(packet.dts))?;
    if time_base.den <= 0 {
        return None;
    }
    u64::try_from(
        i128::from(timestamp) * i128::from(time_base.num) * 1_000 / i128::from(time_base.den),
    )
    .ok()
}

struct Canvas {
    image: RgbImage,
    next: usize,
    tile_size: u32,
    tile_width: u32,
    tile_height: u32,
    scaler: Option<SwsContext>,
    tile: Vec<u8>,
}
impl Canvas {
    fn new(
        width: u32,
        height: u32,
        tile_size: u32,
        tile_width: u32,
        tile_height: u32,
    ) -> Self {
        Self {
            image: RgbImage::new(width, height),
            next: 0,
            tile_size,
            tile_width,
            tile_height,
            scaler: None,
            tile: vec![0; tile_width as usize * tile_height as usize * 3],
        }
    }
    fn push(&mut self, frame: AVFrame, copies: usize) -> Result<(), SpriteGeneratorError> {
        self.scale(&frame)?;
        for _ in 0..copies {
            self.place();
        }
        Ok(())
    }
    fn scale(&mut self, frame: &AVFrame) -> Result<(), SpriteGeneratorError> {
        if self.scaler.is_none() {
            self.scaler = SwsContext::get_context(
                frame.width,
                frame.height,
                frame.format,
                self.tile_width as i32,
                self.tile_height as i32,
                ffi::AV_PIX_FMT_RGB24,
                ffi::SWS_FAST_BILINEAR,
                None,
                None,
                None,
            );
        }
        let scaler = self
            .scaler
            .as_mut()
            .ok_or(SpriteGeneratorError::Generation("could not create scaler"))?;
        let image = AVImage::new(
            ffi::AV_PIX_FMT_RGB24,
            self.tile_width as i32,
            self.tile_height as i32,
            1,
        )
        .ok_or(SpriteGeneratorError::Generation("could not allocate RGB image"))?;
        let mut converted = AVFrameWithImage::new(image);
        scaler.scale_frame(frame, 0, frame.height, &mut converted)?;
        let stride = converted.image().linesizes()[0] as usize;
        let source = converted.image().data()[0];
        if source.is_null() {
            return Err(SpriteGeneratorError::Generation("RGB image has no data"));
        }
        for row in 0..self.tile_height as usize {
            // SAFETY: `converted` contains one RGB row for every output tile row.
            unsafe {
                self.tile[row * self.tile_width as usize * 3..(row + 1) * self.tile_width as usize * 3]
                    .copy_from_slice(slice::from_raw_parts(
                        source.add(row * stride),
                        self.tile_width as usize * 3,
                    ));
            }
        }
        Ok(())
    }
    fn place(&mut self) {
        let x = (self.next as u32 % self.tile_size) * self.tile_width;
        let y = (self.next as u32 / self.tile_size) * self.tile_height;
        for row in 0..self.tile_height as usize {
            let offset = ((y as usize + row) * self.image.width() as usize + x as usize) * 3;
            self.image.as_mut()[offset..offset + self.tile_width as usize * 3].copy_from_slice(
                &self.tile[row * self.tile_width as usize * 3..(row + 1) * self.tile_width as usize * 3],
            );
        }
        self.next += 1;
    }
    fn finish(self) -> Result<Bytes, SpriteGeneratorError> {
        if self.next == 0 {
            return Err(SpriteGeneratorError::Generation("no decoded iframe"));
        }
        let mut output = Vec::new();
        JpegEncoder::new(&mut output)
            .encode_image(&self.image)
            .map_err(|_| SpriteGeneratorError::Generation("could not encode JPEG"))?;
        Ok(Bytes::from(output))
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use opendal::{Operator, services::Memory};
    use relative_path::RelativePath;

    use super::{ExperimentalSpriteGenerator, FrameSelection};
    use crate::track::ResolvedTrack;

    const FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/two-segment-black-white-h264.mp4");

    #[tokio::test]
    async fn input_reads_one_range_for_two_segments() {
        let op = Operator::new(Memory::default()).unwrap();
        let path = RelativePath::new("video.mp4");
        op.write(path.as_str(), Bytes::from_static(FIXTURE))
            .await
            .unwrap();
        let track = ResolvedTrack::discover(&op, path).await.unwrap();
        let generator = ExperimentalSpriteGenerator::new(&op, track.cmaf().unwrap());

        let mut input = generator.input(&[0, 500]).await.unwrap();

        assert!(input.media.recv().await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn previous_keyframe_accepts_requested_cadence() {
        let op = Operator::new(Memory::default()).unwrap();
        let path = RelativePath::new("video.mp4");
        op.write(path.as_str(), Bytes::from_static(FIXTURE))
            .await
            .unwrap();
        let track = ResolvedTrack::discover(&op, path).await.unwrap();
        let generator = ExperimentalSpriteGenerator::new(&op, track.cmaf().unwrap());

        let times = generator
            .source_times(&[499, 500], FrameSelection::PreviousKeyframe)
            .unwrap();

        assert_eq!(times, [499, 500]);
    }

    #[tokio::test]
    async fn jpeg_generates_a_sprite() {
        let op = Operator::new(Memory::default()).unwrap();
        let path = RelativePath::new("video.mp4");
        op.write(path.as_str(), Bytes::from_static(FIXTURE))
            .await
            .unwrap();
        let track = ResolvedTrack::discover(&op, path).await.unwrap();

        let jpeg = ExperimentalSpriteGenerator::new(&op, track.cmaf().unwrap())
            .jpeg(&[0, 500], 2, 16, 16, FrameSelection::PreviousKeyframe)
            .await
            .unwrap();

        assert!(!jpeg.is_empty());
    }
}
