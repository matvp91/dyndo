use super::cmaf_track::CmafTrack;
use super::thumbnail_track::ThumbnailTrack;
use super::vtt_track::VttTrack;

/// A resolved track that can be served as CMAF, raw WebVTT, or thumbnail sprites.
#[derive(Clone)]
pub enum Track {
    Cmaf(CmafTrack),
    Vtt(VttTrack),
    Thumbnail(ThumbnailTrack),
}

impl Track {
    pub fn id(&self) -> &str {
        match self {
            Self::Cmaf(track) => track.id(),
            Self::Vtt(track) => track.id(),
            Self::Thumbnail(track) => track.id(),
        }
    }

    pub fn native_cmaf(&self) -> Option<&CmafTrack> {
        match self {
            Self::Cmaf(track) => Some(track),
            Self::Vtt(_) | Self::Thumbnail(_) => None,
        }
    }

    pub fn vtt(&self) -> Option<&VttTrack> {
        match self {
            Self::Vtt(track) => Some(track),
            _ => None,
        }
    }

    pub fn thumbnail(&self) -> Option<&ThumbnailTrack> {
        match self {
            Self::Thumbnail(track) => Some(track),
            _ => None,
        }
    }

    pub fn source_path(&self) -> Option<&relative_path::RelativePath> {
        match self {
            Self::Cmaf(track) => Some(track.path()),
            Self::Vtt(track) => Some(track.path()),
            Self::Thumbnail(_) => None,
        }
    }
}
