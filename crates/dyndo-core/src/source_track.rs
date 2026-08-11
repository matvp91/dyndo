use relative_path::RelativePath;

use super::cmaf_track::CmafTrack;
use super::web_vtt_track::WebVttTrack;

/// A track backed by an asset source.
#[derive(Clone)]
pub enum SourceTrack {
    Cmaf(CmafTrack),
    Vtt(WebVttTrack),
}

impl SourceTrack {
    pub fn id(&self) -> &str {
        match self {
            Self::Cmaf(track) => track.id(),
            Self::Vtt(track) => track.id(),
        }
    }

    pub fn cmaf(&self) -> Option<&CmafTrack> {
        match self {
            Self::Cmaf(track) => Some(track),
            Self::Vtt(_) => None,
        }
    }

    pub fn source_path(&self) -> &RelativePath {
        match self {
            Self::Cmaf(track) => track.path(),
            Self::Vtt(track) => track.path(),
        }
    }
}
