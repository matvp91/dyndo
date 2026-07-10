use super::codec::{AudioCodec, VideoCodec};

/// The media-type-specific half of a [`CmafHeader`](super::CmafHeader).
#[derive(Debug, Clone, PartialEq)]
pub enum Stream {
    Video(VideoStream),
    Audio(AudioStream),
}

impl Stream {
    /// Sample-entry fourcc (e.g. `"avc1"`, `"mp4a"`), regardless of media type.
    pub fn fourcc(&self) -> &'static str {
        match self {
            Stream::Video(v) => v.codec.fourcc(),
            Stream::Audio(a) => a.codec.fourcc(),
        }
    }

    /// RFC 6381 `codecs` string, regardless of media type.
    pub fn rfc6381(&self) -> String {
        match self {
            Stream::Video(v) => v.codec.rfc6381(),
            Stream::Audio(a) => a.codec.rfc6381(),
        }
    }

    /// ISO-639-2 language, if the track carries one (audio only).
    pub fn language(&self) -> Option<&str> {
        match self {
            Stream::Audio(a) => a.language.as_deref(),
            Stream::Video(_) => None,
        }
    }
}

/// The video-specific fields of a [`Stream`]; `frame_rate` is `(num, den)`.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoStream {
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    pub frame_rate: (u32, u32),
}

/// The audio-specific fields of a [`Stream`].
#[derive(Debug, Clone, PartialEq)]
pub struct AudioStream {
    pub codec: AudioCodec,
    pub sample_rate: u32,
    pub channels: u16,
    pub language: Option<String>,
}
