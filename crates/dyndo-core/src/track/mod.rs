use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use self::thumbnail::ThumbnailTrack;

pub mod cmaf;
pub mod metadata;
mod resolved;
mod source;
pub mod thumbnail;
pub mod timed_text;

pub use resolved::{CmafRepresentationError, ResolvedTrack, TrackResolveError};
pub use source::SourceTrack;

/// The playback category of a resolved track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackType {
    Video,
    Audio,
    Text,
    Thumbnail,
}

impl TrackType {
    /// Returns the filter value for this track type.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Text => "text",
            Self::Thumbnail => "thumbnail",
        }
    }
}

/// The stored or generated form of a resolved track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackFormat {
    Cmaf,
    WebVtt,
    Thumbnail,
}

impl TrackFormat {
    /// Returns the filter value for this track format.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cmaf => "cmaf",
            Self::WebVtt => "webvtt",
            Self::Thumbnail => "thumbnail",
        }
    }
}

/// A track stored in an asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

    /// Returns the serialized source discriminator used in `asset.json`.
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
