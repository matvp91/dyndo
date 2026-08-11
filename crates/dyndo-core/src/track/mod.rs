use serde::{Deserialize, Serialize};

use self::thumbnail::ThumbnailTrack;

pub mod cmaf;
pub mod metadata;
mod source;
pub mod thumbnail;
pub mod timed_text;

pub use source::{ResolvedSourceTrack, SourceResolveError, SourceTrack};

/// A track stored in an asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Track {
    Thumbnail(ThumbnailTrack),
    #[serde(untagged)]
    Source(SourceTrack),
}

impl Track {
    pub fn id(&self) -> &str {
        match self {
            Self::Source(track) => track.id(),
            Self::Thumbnail(track) => &track.id,
        }
    }

    pub fn asset_type(&self) -> &'static str {
        match self {
            Self::Source(track) => track.asset_type(),
            Self::Thumbnail(_) => "thumbnail",
        }
    }

    pub fn source(&self) -> Option<&SourceTrack> {
        match self {
            Self::Source(track) => Some(track),
            Self::Thumbnail(_) => None,
        }
    }

    pub fn source_mut(&mut self) -> Option<&mut SourceTrack> {
        match self {
            Self::Source(track) => Some(track),
            Self::Thumbnail(_) => None,
        }
    }

    pub fn thumbnail(&self) -> Option<&ThumbnailTrack> {
        match self {
            Self::Source(_) => None,
            Self::Thumbnail(track) => Some(track),
        }
    }
}
