use relative_path::RelativePath;

use super::web_vtt_track::WebVttTrack;

/// A source track represented by timed-text documents.
#[derive(Clone)]
pub enum TimedTextTrack {
    WebVtt(WebVttTrack),
}

impl TimedTextTrack {
    pub fn id(&self) -> &str {
        match self {
            Self::WebVtt(track) => track.id(),
        }
    }

    pub fn source_path(&self) -> &RelativePath {
        match self {
            Self::WebVtt(track) => track.path(),
        }
    }
}
