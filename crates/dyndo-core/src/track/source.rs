use language_tags::LanguageTag;
use opendal::Operator;
use relative_path::{RelativePath, RelativePathBuf};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::cmaf::{CmafKind, CmafTrack, ResolvedCmafTrack};
use super::metadata::{AudioMetadata, TextMetadata, VideoMetadata};
use super::timed_text::{ResolvedTimedTextTrack, TimedTextTrack};
use crate::role::Role;

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

    pub fn cmaf_kind(&self) -> Option<&CmafKind> {
        match self {
            Self::Cmaf(track) => Some(&track.kind),
            Self::TimedText(_) => None,
        }
    }

    pub fn video_metadata(&self) -> Option<&VideoMetadata> {
        match self {
            Self::Cmaf(track) => track.kind.video(),
            Self::TimedText(_) => None,
        }
    }

    pub fn audio_metadata(&self) -> Option<&AudioMetadata> {
        match self {
            Self::Cmaf(track) => track.kind.audio(),
            Self::TimedText(_) => None,
        }
    }

    pub fn language(&self) -> Option<&LanguageTag> {
        match self {
            Self::Cmaf(track) => track.kind.language(),
            Self::TimedText(track) => Some(&track.format.text().language),
        }
    }

    pub fn role(&self) -> Option<Role> {
        match self {
            Self::Cmaf(track) => track.kind.role(),
            Self::TimedText(track) => track.format.text().role,
        }
    }

    pub fn language_and_role_mut(&mut self) -> Option<(&mut LanguageTag, &mut Option<Role>)> {
        match self {
            Self::Cmaf(track) => track.kind.language_and_role_mut(),
            Self::TimedText(track) => {
                let metadata = track.format.text_mut();
                Some((&mut metadata.language, &mut metadata.role))
            }
        }
    }

    pub fn asset_type(&self) -> &'static str {
        match self {
            Self::Cmaf(track) => track.kind.content_type(),
            Self::TimedText(track) => track.format.asset_type(),
        }
    }

    /// Resolves this configured source from storage.
    pub async fn resolve(
        &self,
        op: &Operator,
        path: &RelativePath,
    ) -> Result<ResolvedSourceTrack, SourceResolveError> {
        match self {
            Self::Cmaf(track) => track
                .resolve(op, path)
                .await
                .map(ResolvedSourceTrack::Cmaf)
                .map_err(Into::into),
            Self::TimedText(track) => track
                .resolve(op, path)
                .await
                .map(ResolvedSourceTrack::TimedText)
                .map_err(Into::into),
        }
    }

    pub(crate) fn from_resolved(track: &ResolvedSourceTrack, path: RelativePathBuf) -> Self {
        match track {
            ResolvedSourceTrack::TimedText(track) => Self::TimedText(TimedTextTrack {
                id: track.id().to_string(),
                path,
                format: track.format().clone(),
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

/// A track backed by a resolved asset source.
#[derive(Clone)]
pub enum ResolvedSourceTrack {
    Cmaf(ResolvedCmafTrack),
    TimedText(ResolvedTimedTextTrack),
}

impl ResolvedSourceTrack {
    /// Discovers and resolves an unconfigured source.
    pub async fn discover(op: &Operator, path: &RelativePath) -> Result<Self, SourceResolveError> {
        let id = source_id(path);
        if path.as_str().ends_with(".vtt") {
            return ResolvedTimedTextTrack::from_web_vtt_source(
                op,
                path,
                id,
                TextMetadata::default(),
            )
            .await
            .map(Self::TimedText)
            .map_err(Into::into);
        }

        ResolvedCmafTrack::discover(op, path, id)
            .await
            .map(Self::Cmaf)
            .map_err(Into::into)
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Cmaf(track) => track.id(),
            Self::TimedText(track) => track.id(),
        }
    }

    pub fn cmaf(&self) -> Option<&ResolvedCmafTrack> {
        match self {
            Self::Cmaf(track) => Some(track),
            Self::TimedText(_) => None,
        }
    }

    /// Returns the stored source path, or `None` for an in-memory CMAF representation.
    pub fn source_path(&self) -> Option<&RelativePath> {
        match self {
            Self::Cmaf(track) => track.source_path(),
            Self::TimedText(track) => Some(track.source_path()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SourceResolveError {
    #[error(transparent)]
    Cmaf(#[from] super::cmaf::CmafError),
    #[error(transparent)]
    TimedText(#[from] super::timed_text::TimedTextError),
}

fn source_id(path: &RelativePath) -> String {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, path.as_str().as_bytes()).to_string()
}
