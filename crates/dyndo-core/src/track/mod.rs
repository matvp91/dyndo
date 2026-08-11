use relative_path::RelativePath;

use self::cmaf::ResolvedCmafTrack;
use self::timed_text::ResolvedTimedTextTrack;

pub mod cmaf;
mod config;
pub mod kind;
pub mod thumbnail;
pub mod timed_text;

pub use config::{CmafTrack, SourceTrack, ThumbnailTrack, TimedTextTrack, Track};

/// A track backed by an asset source.
#[derive(Clone)]
pub enum ResolvedSourceTrack {
    Cmaf(ResolvedCmafTrack),
    TimedText(ResolvedTimedTextTrack),
}

impl ResolvedSourceTrack {
    pub fn id(&self) -> &str {
        match self {
            Self::Cmaf(track) => track.id(),
            Self::TimedText(track) => track.id(),
        }
    }

    pub fn cmaf(&self) -> Option<&ResolvedCmafTrack> {
        match self {
            Self::Cmaf(track) => Some(track),
            Self::TimedText(_) => None,
        }
    }

    pub fn source_path(&self) -> &RelativePath {
        match self {
            Self::Cmaf(track) => track.path(),
            Self::TimedText(track) => track.path(),
        }
    }
}
