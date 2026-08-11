use relative_path::RelativePath;

use self::cmaf::CmafTrack;
use self::timed_text::TimedTextTrack;

pub mod cmaf;
pub mod thumbnail;
pub mod timed_text;

/// A track backed by an asset source.
#[derive(Clone)]
pub enum SourceTrack {
    Cmaf(CmafTrack),
    TimedText(TimedTextTrack),
}

impl SourceTrack {
    pub fn id(&self) -> &str {
        match self {
            Self::Cmaf(track) => track.id(),
            Self::TimedText(track) => track.id(),
        }
    }

    pub fn cmaf(&self) -> Option<&CmafTrack> {
        match self {
            Self::Cmaf(track) => Some(track),
            Self::TimedText(_) => None,
        }
    }

    pub fn source_path(&self) -> &RelativePath {
        match self {
            Self::Cmaf(track) => track.path(),
            Self::TimedText(track) => track.source_path(),
        }
    }
}
