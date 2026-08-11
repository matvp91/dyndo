use relative_path::RelativePathBuf;
use serde::{Deserialize, Serialize};

use super::cmaf_track_descriptor::CmafTrackDescriptor;
use super::cmaf_track_kind::{AudioKind, CmafTrackKind, TextKind, VideoKind};
use super::source_track::SourceTrack;
use super::thumbnail_track_descriptor::ThumbnailTrackDescriptor;
use super::web_vtt_track_descriptor::WebVttTrackDescriptor;

/// A track configuration in an asset descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TrackDescriptor {
    Video(CmafTrackDescriptor<VideoKind>),
    Audio(CmafTrackDescriptor<AudioKind>),
    Text(CmafTrackDescriptor<TextKind>),
    #[serde(rename = "vtt")]
    WebVtt(WebVttTrackDescriptor),
    Thumbnail(ThumbnailTrackDescriptor),
}

impl TrackDescriptor {
    pub fn id(&self) -> &str {
        match self {
            Self::Video(track) => &track.id,
            Self::Audio(track) => &track.id,
            Self::Text(track) => &track.id,
            Self::WebVtt(track) => &track.id,
            Self::Thumbnail(track) => &track.id,
        }
    }

    pub fn source_path(&self) -> Option<&RelativePathBuf> {
        match self {
            Self::Video(track) => Some(&track.path),
            Self::Audio(track) => Some(&track.path),
            Self::Text(track) => Some(&track.path),
            Self::WebVtt(track) => Some(&track.path),
            Self::Thumbnail(_) => None,
        }
    }

    pub fn cmaf_kind(&self) -> Option<CmafTrackKind> {
        match self {
            Self::Video(track) => Some(CmafTrackKind::Video(track.kind.clone())),
            Self::Audio(track) => Some(CmafTrackKind::Audio(track.kind.clone())),
            Self::Text(track) => Some(CmafTrackKind::Text(track.kind.clone())),
            Self::WebVtt(_) | Self::Thumbnail(_) => None,
        }
    }

    pub fn content_type(&self) -> &'static str {
        match self {
            Self::Video(_) => "video",
            Self::Audio(_) => "audio",
            Self::Text(_) => "text",
            Self::WebVtt(_) => "vtt",
            Self::Thumbnail(_) => "thumbnail",
        }
    }

    pub fn thumbnail(&self) -> Option<&ThumbnailTrackDescriptor> {
        match self {
            Self::Thumbnail(track) => Some(track),
            _ => None,
        }
    }

    pub(super) fn from_source_track(track: &SourceTrack, path: RelativePathBuf) -> Self {
        match track {
            SourceTrack::WebVtt(track) => Self::WebVtt(WebVttTrackDescriptor {
                id: track.id().to_string(),
                path,
                kind: track.kind().clone(),
            }),
            SourceTrack::Cmaf(track) => {
                let id = track.id().to_string();
                let codec = track.codec().rfc6381();
                match track.kind() {
                    CmafTrackKind::Video(kind) => Self::Video(CmafTrackDescriptor {
                        id,
                        path,
                        codec,
                        kind: kind.clone(),
                    }),
                    CmafTrackKind::Audio(kind) => Self::Audio(CmafTrackDescriptor {
                        id,
                        path,
                        codec,
                        kind: kind.clone(),
                    }),
                    CmafTrackKind::Text(kind) => Self::Text(CmafTrackDescriptor {
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
