//! Experimental, single-pass JPEG sprite generation.

use std::ops::Range;
use std::slice;

use bytes::Bytes;
use image::{RgbImage, codecs::jpeg::JpegEncoder};
use opendal::Operator;
use rsmpeg::avcodec::AVCodecContext;
use rsmpeg::avformat::{AVFormatContextInput, AVIOContextContainer, AVIOContextCustom};
use rsmpeg::avutil::{AVFrame, AVFrameWithImage, AVImage, AVMem, AVRational};
use rsmpeg::error::RsmpegError;
use rsmpeg::{ffi, swscale::SwsContext};

use crate::track::cmaf::{CmafReadError, ResolvedCmafTrack};

/// Selects the source frame for a sprite tile.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameSelection {
    /// Return the displayed frame at the requested time.
    #[default]
    Exact,
    /// Return the random-access frame at the start of the containing CMAF segment.
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
    #[error("could not generate sprite")]
    Generation,
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

    /// Resolves the source timestamps used by the requested selection policy.
    pub(crate) fn source_times(
        &self,
        times: &[u64],
        selection: FrameSelection,
    ) -> Option<Vec<u64>> {
        times
            .iter()
            .map(|&time| match selection {
                FrameSelection::Exact => Some(time),
                FrameSelection::PreviousKeyframe => self
                    .track
                    .segments()
                    .iter()
                    .find(|segment| segment.start_time() <= time && time < segment.end_time())
                    .map(|segment| segment.start_time()),
            })
            .collect()
    }

    /// Reads the init segment and one contiguous media window.
    ///
    /// This is intentionally the only storage-facing operation in the proof of concept. The next
    /// iteration replaces `media` with a bounded byte stream feeding one FFmpeg decoder.
    pub(crate) async fn input(&self, times: &[u64]) -> Result<SpriteInput, CmafReadError> {
        let media_range = self.media_range(times).ok_or(CmafReadError::Range)?;
        let (initialization, media) = tokio::try_join!(
            self.track
                .read_range(self.op, self.track.init_segment().byte_range()),
            self.track.read_range(self.op, media_range),
        )?;
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
        let targets = self
            .source_times(times, selection)
            .ok_or(SpriteGeneratorError::Generation)?;
        let input = self.input(times).await?;
        let width = tile_size
            .checked_mul(tile_width)
            .ok_or(SpriteGeneratorError::Generation)?;
        let height = tile_size
            .checked_mul(tile_height)
            .ok_or(SpriteGeneratorError::Generation)?;
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
        .map_err(|_| SpriteGeneratorError::Generation)?
    }
}

/// The CMAF bytes a single sprite decoder receives.
pub(crate) struct SpriteInput {
    pub(crate) initialization: Bytes,
    pub(crate) media: Bytes,
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
    let mut bytes = Vec::with_capacity(input.initialization.len() + input.media.len());
    bytes.extend_from_slice(&input.initialization);
    bytes.extend_from_slice(&input.media);
    let mut position = 0;
    let io = AVIOContextCustom::alloc_context(
        AVMem::new(4_096),
        false,
        Vec::new(),
        Some(Box::new(move |_, output| {
            let end = bytes.len().min(position + output.len());
            if end == position {
                return ffi::AVERROR_EOF;
            }
            output[..end - position].copy_from_slice(&bytes[position..end]);
            let length = end - position;
            position = end;
            length as i32
        })),
        None,
        None,
    );
    let mut format = AVFormatContextInput::from_io_context(AVIOContextContainer::Custom(io))?;
    let (stream_index, decoder) = format
        .find_best_stream(ffi::AVMEDIA_TYPE_VIDEO)?
        .ok_or(SpriteGeneratorError::Generation)?;
    let stream = &format.streams()[stream_index];
    let mut context = AVCodecContext::new(&decoder);
    context.apply_codecpar(&stream.codecpar())?;
    context.open(None)?;
    let mut canvas = Canvas::new(
        width,
        height,
        tile_size,
        tile_width,
        tile_height,
        targets,
        stream.time_base,
    );
    while let Some(packet) = format.read_packet()? {
        if packet.stream_index != stream_index as i32 {
            continue;
        }
        context.send_packet(Some(&packet))?;
        receive(&mut context, &mut canvas)?;
    }
    context.send_packet(None)?;
    receive(&mut context, &mut canvas)?;
    canvas.finish()
}

fn receive(decoder: &mut AVCodecContext, canvas: &mut Canvas) -> Result<(), SpriteGeneratorError> {
    loop {
        match decoder.receive_frame() {
            Ok(frame) => canvas.push(frame)?,
            Err(RsmpegError::DecoderDrainError | RsmpegError::DecoderFlushedError) => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
}

struct Canvas {
    image: RgbImage,
    targets: Vec<u64>,
    next: usize,
    candidate: Option<AVFrame>,
    time_base: AVRational,
    tile_size: u32,
    tile_width: u32,
    tile_height: u32,
}
impl Canvas {
    fn new(
        width: u32,
        height: u32,
        tile_size: u32,
        tile_width: u32,
        tile_height: u32,
        targets: Vec<u64>,
        time_base: AVRational,
    ) -> Self {
        Self {
            image: RgbImage::new(width, height),
            targets,
            next: 0,
            candidate: None,
            time_base,
            tile_size,
            tile_width,
            tile_height,
        }
    }
    fn push(&mut self, frame: AVFrame) -> Result<(), SpriteGeneratorError> {
        let Some(time) = frame_time(&frame, self.time_base) else {
            self.candidate = Some(frame);
            return Ok(());
        };
        while self.next < self.targets.len() && time > self.targets[self.next] {
            self.place()?;
        }
        self.candidate = Some(frame);
        Ok(())
    }
    fn place(&mut self) -> Result<(), SpriteGeneratorError> {
        let frame = self
            .candidate
            .as_ref()
            .ok_or(SpriteGeneratorError::Generation)?;
        let mut scaler = SwsContext::get_context(
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
        )
        .ok_or(SpriteGeneratorError::Generation)?;
        let image = AVImage::new(
            ffi::AV_PIX_FMT_RGB24,
            self.tile_width as i32,
            self.tile_height as i32,
            1,
        )
        .ok_or(SpriteGeneratorError::Generation)?;
        let mut converted = AVFrameWithImage::new(image);
        scaler.scale_frame(frame, 0, frame.height, &mut converted)?;
        let x = (self.next as u32 % self.tile_size) * self.tile_width;
        let y = (self.next as u32 / self.tile_size) * self.tile_height;
        let stride = converted.image().linesizes()[0] as usize;
        let source = converted.image().data()[0];
        if source.is_null() {
            return Err(SpriteGeneratorError::Generation);
        }
        for row in 0..self.tile_height as usize {
            let offset = ((y as usize + row) * self.image.width() as usize + x as usize) * 3;
            unsafe {
                self.image.as_mut()[offset..offset + self.tile_width as usize * 3].copy_from_slice(
                    slice::from_raw_parts(source.add(row * stride), self.tile_width as usize * 3),
                );
            }
        }
        self.next += 1;
        Ok(())
    }
    fn finish(mut self) -> Result<Bytes, SpriteGeneratorError> {
        while self.next < self.targets.len() {
            self.place()?;
        }
        let mut output = Vec::new();
        JpegEncoder::new(&mut output)
            .encode_image(&self.image)
            .map_err(|_| SpriteGeneratorError::Generation)?;
        Ok(Bytes::from(output))
    }
}
fn frame_time(frame: &AVFrame, time_base: AVRational) -> Option<u64> {
    if frame.best_effort_timestamp == ffi::AV_NOPTS_VALUE || time_base.den <= 0 {
        return None;
    }
    u64::try_from(
        i128::from(frame.best_effort_timestamp) * i128::from(time_base.num) * 1_000
            / i128::from(time_base.den),
    )
    .ok()
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

        let input = generator.input(&[0, 500]).await.unwrap();

        assert!(!input.media.is_empty());
    }

    #[tokio::test]
    async fn previous_keyframe_uses_the_segment_start() {
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

        assert_eq!(times, [0, 500]);
    }
}
