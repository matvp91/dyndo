use crate::asset::kind::{AudioKind, TextKind, ThumbnailKind, VideoKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmafTrackKind {
    Video(VideoKind),
    Audio(AudioKind),
    Text(TextKind),
}

/// The document format carried by a timed-text source track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimedTextKind {
    WebVtt(TextKind),
}

impl TimedTextKind {
    /// Returns the text metadata shared by all timed-text formats.
    pub fn text(&self) -> &TextKind {
        match self {
            Self::WebVtt(kind) => kind,
        }
    }

    /// Returns whether this source is a WebVTT document.
    pub fn is_web_vtt(&self) -> bool {
        matches!(self, Self::WebVtt(_))
    }
}

/// The output type of a synthetic track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntheticTrackKind {
    Thumbnail(ThumbnailKind),
}

impl CmafTrackKind {
    /// Returns the DASH media content type represented by a track of this kind.
    pub fn content_type(&self) -> &'static str {
        match self {
            Self::Video(_) => "video",
            Self::Audio(_) => "audio",
            Self::Text(_) => "text",
        }
    }

    /// Returns the media type of the CMAF representation of a track of this kind.
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Video(_) => "video/mp4",
            Self::Audio(_) => "audio/mp4",
            Self::Text(_) => "application/mp4",
        }
    }
}
