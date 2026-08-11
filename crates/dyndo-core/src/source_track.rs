use relative_path::RelativePath;

use super::cmaf_track::CmafTrack;
use super::timed_text_track::TimedTextTrack;

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
