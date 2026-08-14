use language_tags::LanguageTag;
use opendal::Operator;
use relative_path::{RelativePath, RelativePathBuf};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::cmaf::CmafTrack;
use super::resolved::{ResolvedTrack, TrackResolveError};
use super::timed_text::TimedTextTrack;
use crate::drm::Cpix;
use crate::role::Role;

/// A track backed by a file stored with an asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

    pub fn language_and_role_mut(&mut self) -> Option<(&mut LanguageTag, &mut Option<Role>)> {
        match self {
            Self::Cmaf(track) => track.metadata.language_and_role_mut(),
            Self::TimedText(track) => {
                Some((&mut track.metadata.language, &mut track.metadata.role))
            }
        }
    }

    /// Resolves this configured source from storage.
    pub async fn resolve(
        &self,
        op: &Operator,
        path: &RelativePath,
        cpix: Option<&Cpix>,
    ) -> Result<ResolvedTrack, TrackResolveError> {
        match self {
            Self::Cmaf(track) => {
                let mut track = track.resolve(op, path).await?;
                if let Some(cpix) = cpix {
                    track.protect(cpix)?;
                }
                Ok(ResolvedTrack::Cmaf(Arc::new(track)))
            }
            Self::TimedText(track) => track
                .resolve(op, path)
                .await
                .map(ResolvedTrack::TimedText)
                .map_err(Into::into),
        }
    }

    pub(crate) fn from_resolved(track: &ResolvedTrack, path: RelativePathBuf) -> Option<Self> {
        match track {
            ResolvedTrack::TimedText(track) => Some(Self::TimedText(TimedTextTrack {
                id: track.id().to_string(),
                path,
                format: track.format().clone(),
                metadata: track.metadata().clone(),
            })),
            ResolvedTrack::Cmaf(track) => Some(Self::Cmaf(CmafTrack {
                id: track.id().to_string(),
                path,
                codec: track.codec().rfc6381(),
                metadata: track.metadata().clone(),
            })),
            ResolvedTrack::Thumbnail(_) => None,
        }
    }
}
use std::sync::Arc;
