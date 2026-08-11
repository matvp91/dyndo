use language_tags::LanguageTag;
use relative_path::{RelativePath, RelativePathBuf};
use serde::{Deserialize, Serialize};

use super::kind::{AudioKind, VideoKind};
use crate::role::Role;
use crate::track::SourceTrack;
use crate::track::kind::{CmafTrackKind, SyntheticTrackKind, TimedTextKind};

/// A CMAF source-track configuration in an asset descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CmafTrackDescriptor {
    pub id: String,
    /// Path relative to the asset descriptor.
    pub(super) path: RelativePathBuf,
    pub codec: String,
    #[serde(flatten)]
    pub kind: CmafTrackKind,
}

/// A timed-text source-track configuration in an asset descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimedTextTrackDescriptor {
    pub id: String,
    /// Path relative to the asset descriptor.
    pub(super) path: RelativePathBuf,
    #[serde(flatten)]
    pub kind: TimedTextKind,
}

/// A source-track configuration in an asset descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SourceTrackDescriptor {
    Cmaf(CmafTrackDescriptor),
    TimedText(TimedTextTrackDescriptor),
}

impl SourceTrackDescriptor {
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

    pub fn cmaf_kind(&self) -> Option<&CmafTrackKind> {
        match self {
            Self::Cmaf(track) => Some(&track.kind),
            Self::TimedText(_) => None,
        }
    }

    pub fn codec(&self) -> Option<&str> {
        match self {
            Self::Cmaf(track) => Some(&track.codec),
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

    pub(super) fn from_source_track(track: &SourceTrack, path: RelativePathBuf) -> Self {
        match track {
            SourceTrack::TimedText(track) => Self::TimedText(TimedTextTrackDescriptor {
                id: track.id().to_string(),
                path,
                kind: track.kind().clone(),
            }),
            SourceTrack::Cmaf(track) => Self::Cmaf(CmafTrackDescriptor {
                id: track.id().to_string(),
                path,
                codec: track.codec().rfc6381(),
                kind: track.kind().clone(),
            }),
        }
    }
}

/// A synthetic-track configuration in an asset descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntheticTrackDescriptor {
    /// Identifier used to address the synthetic track.
    pub id: String,
    #[serde(flatten)]
    pub kind: SyntheticTrackKind,
}

impl SyntheticTrackDescriptor {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn asset_type(&self) -> &'static str {
        self.kind.asset_type()
    }

    pub fn thumbnail(&self) -> Option<&super::kind::ThumbnailKind> {
        self.kind.thumbnail()
    }
}

/// A track configuration in an asset descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TrackDescriptor {
    Source(SourceTrackDescriptor),
    Synthetic(SyntheticTrackDescriptor),
}

impl TrackDescriptor {
    pub fn id(&self) -> &str {
        match self {
            Self::Source(track) => track.id(),
            Self::Synthetic(track) => track.id(),
        }
    }

    pub fn asset_type(&self) -> &'static str {
        match self {
            Self::Source(track) => track.asset_type(),
            Self::Synthetic(track) => track.asset_type(),
        }
    }

    pub fn source(&self) -> Option<&SourceTrackDescriptor> {
        match self {
            Self::Source(track) => Some(track),
            Self::Synthetic(_) => None,
        }
    }

    pub fn source_mut(&mut self) -> Option<&mut SourceTrackDescriptor> {
        match self {
            Self::Source(track) => Some(track),
            Self::Synthetic(_) => None,
        }
    }

    pub fn synthetic(&self) -> Option<&SyntheticTrackDescriptor> {
        match self {
            Self::Source(_) => None,
            Self::Synthetic(track) => Some(track),
        }
    }
}
