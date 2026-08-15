use bytes::Bytes;
use rsmpeg::avcodec::AVCodecContext;
use rsmpeg::avformat::{AVFormatContextInput, AVIOContextContainer, AVIOContextCustom};
use rsmpeg::avutil::{AVFrame, AVMem, AVRational};
use rsmpeg::error::RsmpegError;
use rsmpeg::ffi;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub(super) struct FrameDecoderError(String);

impl FrameDecoderError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl From<rsmpeg::error::RsmpegError> for FrameDecoderError {
    fn from(error: rsmpeg::error::RsmpegError) -> Self {
        Self::new(format!("FFmpeg failed: {error}"))
    }
}

pub(super) struct FrameDecoder {
    format: AVFormatContextInput,
    decoder: AVCodecContext,
    stream_index: usize,
    time_base: AVRational,
    flushed: bool,
}

impl FrameDecoder {
    pub(super) fn new(
        mut chunks: tokio::sync::mpsc::Receiver<Bytes>,
    ) -> Result<Self, FrameDecoderError> {
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
            .ok_or_else(|| FrameDecoderError::new("video stream not found"))?;
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

    pub(super) fn frame_at(&mut self, target: u64) -> Result<Option<AVFrame>, FrameDecoderError> {
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
