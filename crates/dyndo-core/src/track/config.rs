use language_tags::LanguageTag;
use relative_path::{RelativePath, RelativePathBuf};
use serde::{Deserialize, Serialize};

use super::ResolvedSourceTrack;
use super::kind::{AudioKind, CmafTrackKind, TimedTextKind, VideoKind};
use crate::role::Role;

/// A CMAF track stored in an asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CmafTrack {
    pub id: String,
    /// Path relative to the asset file.
    pub(super) path: RelativePathBuf,
    pub codec: String,
    #[serde(flatten)]
    pub kind: CmafTrackKind,
}

/// A timed-text track stored in an asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimedTextTrack {
    pub id: String,
    /// Path relative to the asset file.
    pub(super) path: RelativePathBuf,
    #[serde(flatten)]
    pub kind: TimedTextKind,
}

/// A track backed by a file stored with an asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SourceTrack {
    Cmaf(CmafTrack),
    TimedText(TimedTextTrack),
}

impl SourceTrack {
    pub fn id(&self) -> &str {
        match self {
            Self::Cmaf(track) => &track.id,
            Self::TimedText(track) => &track.id,
        }
    }

    pub fn source_path(&self) -> &RelativePath {
        match self {
            Self::Cmaf(track) => &track.path,
            Self::TimedText(track) => &track.path,
        }
    }

    pub fn codec(&self) -> Option<&str> {
        match self {
            Self::Cmaf(track) => Some(&track.codec),
            Self::TimedText(_) => None,
        }
    }

    pub fn cmaf_kind(&self) -> Option<&CmafTrackKind> {
        match self {
            Self::Cmaf(track) => Some(&track.kind),
            Self::TimedText(_) => None,
        }
    }

    pub fn video_kind(&self) -> Option<&VideoKind> {
        match self {
            Self::Cmaf(track) => track.kind.video(),
            Self::TimedText(_) => None,
        }
    }

    pub fn audio_kind(&self) -> Option<&AudioKind> {
        match self {
            Self::Cmaf(track) => track.kind.audio(),
            Self::TimedText(_) => None,
        }
    }

    pub fn language(&self) -> Option<&LanguageTag> {
        match self {
            Self::Cmaf(track) => track.kind.language(),
            Self::TimedText(track) => Some(&track.kind.text().language),
        }
    }

    pub fn role(&self) -> Option<Role> {
        match self {
            Self::Cmaf(track) => track.kind.role(),
            Self::TimedText(track) => track.kind.text().role,
        }
    }

    pub fn language_and_role_mut(&mut self) -> Option<(&mut LanguageTag, &mut Option<Role>)> {
        match self {
            Self::Cmaf(track) => track.kind.language_and_role_mut(),
            Self::TimedText(track) => {
                let kind = track.kind.text_mut();
                Some((&mut kind.language, &mut kind.role))
            }
        }
    }

    pub fn asset_type(&self) -> &'static str {
        match self {
            Self::Cmaf(track) => track.kind.content_type(),
            Self::TimedText(track) => track.kind.asset_type(),
        }
    }

    pub(crate) fn from_resolved(track: &ResolvedSourceTrack, path: RelativePathBuf) -> Self {
        match track {
            ResolvedSourceTrack::TimedText(track) => Self::TimedText(TimedTextTrack {
                id: track.id().to_string(),
                path,
                kind: track.kind().clone(),
            }),
            ResolvedSourceTrack::Cmaf(track) => Self::Cmaf(CmafTrack {
                id: track.id().to_string(),
                path,
                codec: track.codec().rfc6381(),
                kind: track.kind().clone(),
            }),
        }
    }
}

/// A thumbnail track generated from source video when requested.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThumbnailTrack {
    /// Identifier used to address the thumbnail track.
    pub id: String,
    #[serde(rename = "type")]
    track_type: ThumbnailType,
    /// Thumbnails per sprite row and column.
    pub tile_size: u32,
    /// Width of the complete sprite image, in pixels.
    pub width: u32,
    /// Milliseconds between adjacent thumbnails.
    pub step: u32,
}

impl ThumbnailTrack {
    pub fn new(id: String, tile_size: u32, width: u32, step: u32) -> Self {
        Self {
            id,
            track_type: ThumbnailType::Thumbnail,
            tile_size,
            width,
            step,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ThumbnailType {
    Thumbnail,
}

/// A track stored in an asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Track {
    Source(SourceTrack),
    Thumbnail(ThumbnailTrack),
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
