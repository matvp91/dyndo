use relative_path::{RelativePath, RelativePathBuf};
use serde::{Deserialize, Serialize};

use super::descriptor::{
    AudioTrackDescriptor, TextTrackDescriptor, ThumbnailTrackDescriptor, VideoTrackDescriptor,
    WebVttTrackDescriptor,
};
use crate::track::SourceTrack;
use crate::track::kind::{CmafTrackKind, TimedTextKind};

/// A source-track configuration in an asset descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SourceTrackDescriptor {
    Video(VideoTrackDescriptor),
    Audio(AudioTrackDescriptor),
    Text(TextTrackDescriptor),
    WebVtt(WebVttTrackDescriptor),
}

impl SourceTrackDescriptor {
    pub fn id(&self) -> &str {
        match self {
            Self::Video(track) => &track.id,
            Self::Audio(track) => &track.id,
            Self::Text(track) => &track.id,
            Self::WebVtt(track) => &track.id,
        }
    }

    pub fn source_path(&self) -> &RelativePath {
        match self {
            Self::Video(track) => &track.path,
            Self::Audio(track) => &track.path,
            Self::Text(track) => &track.path,
            Self::WebVtt(track) => &track.path,
        }
    }

    pub fn cmaf_kind(&self) -> Option<CmafTrackKind> {
        match self {
            Self::Video(track) => Some(CmafTrackKind::Video(track.kind.clone())),
            Self::Audio(track) => Some(CmafTrackKind::Audio(track.kind.clone())),
            Self::Text(track) => Some(CmafTrackKind::Text(track.kind.clone())),
            Self::WebVtt(_) => None,
        }
    }

    pub fn asset_type(&self) -> &'static str {
        match self {
            Self::Video(_) => "video",
            Self::Audio(_) => "audio",
            Self::Text(_) => "text",
            Self::WebVtt(_) => "webvtt",
        }
    }

    pub(super) fn from_source_track(track: &SourceTrack, path: RelativePathBuf) -> Self {
        match track {
            SourceTrack::TimedText(track) => match track.kind() {
                TimedTextKind::WebVtt(kind) => Self::WebVtt(WebVttTrackDescriptor {
                    id: track.id().to_string(),
                    path,
                    kind: kind.clone(),
                }),
            },
            SourceTrack::Cmaf(track) => {
                let id = track.id().to_string();
                let codec = track.codec().rfc6381();
                match track.kind() {
                    CmafTrackKind::Video(kind) => Self::Video(VideoTrackDescriptor {
                        id,
                        path,
                        codec,
                        kind: kind.clone(),
                    }),
                    CmafTrackKind::Audio(kind) => Self::Audio(AudioTrackDescriptor {
                        id,
                        path,
                        codec,
                        kind: kind.clone(),
                    }),
                    CmafTrackKind::Text(kind) => Self::Text(TextTrackDescriptor {
                        id,
                        path,
                        codec,
                        kind: kind.clone(),
                    }),
                }
            }
        }
    }
}

/// A synthetic-track configuration in an asset descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SyntheticTrackDescriptor {
    Thumbnail(ThumbnailTrackDescriptor),
}

impl SyntheticTrackDescriptor {
    pub fn id(&self) -> &str {
        match self {
            Self::Thumbnail(track) => &track.id,
        }
    }

    pub fn asset_type(&self) -> &'static str {
        match self {
            Self::Thumbnail(_) => "thumbnail",
        }
    }

    pub fn thumbnail(&self) -> &ThumbnailTrackDescriptor {
        match self {
            Self::Thumbnail(track) => track,
        }
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
