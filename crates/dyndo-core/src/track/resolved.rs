use language_tags::LanguageTag;
use opendal::Operator;
use relative_path::RelativePath;
use uuid::Uuid;

use super::cmaf::{CmafError, ResolvedCmafTrack};
use super::metadata::{AudioMetadata, TextMetadata, VideoMetadata};
use super::thumbnail::ResolvedThumbnailTrack;
use super::timed_text::{ResolvedTimedTextTrack, TimedTextError, WebVttPackageError};
use crate::role::Role;
use crate::segment_options::SegmentOptions;

/// One configured asset track whose source or dependencies have been resolved.
#[derive(Clone)]
pub enum ResolvedTrack {
    Cmaf(ResolvedCmafTrack),
    TimedText(ResolvedTimedTextTrack),
    Thumbnail(ResolvedThumbnailTrack),
}

impl ResolvedTrack {
    /// Discovers and resolves an unconfigured source.
    pub async fn discover(op: &Operator, path: &RelativePath) -> Result<Self, TrackResolveError> {
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
            Self::Thumbnail(track) => track.id(),
        }
    }

    pub fn asset_type(&self) -> &'static str {
        match self {
            Self::Cmaf(track) => track.kind().content_type(),
            Self::TimedText(track) => track.format().asset_type(),
            Self::Thumbnail(_) => "thumbnail",
        }
    }

    pub fn codec(&self) -> Option<String> {
        match self {
            Self::Cmaf(track) => Some(track.codec().rfc6381()),
            Self::TimedText(_) | Self::Thumbnail(_) => None,
        }
    }

    pub fn video_metadata(&self) -> Option<&VideoMetadata> {
        self.cmaf().and_then(|track| track.kind().video())
    }

    pub fn audio_metadata(&self) -> Option<&AudioMetadata> {
        self.cmaf().and_then(|track| track.kind().audio())
    }

    pub fn language(&self) -> Option<&LanguageTag> {
        match self {
            Self::Cmaf(track) => track.kind().language(),
            Self::TimedText(track) => Some(&track.format().text().language),
            Self::Thumbnail(_) => None,
        }
    }

    pub fn role(&self) -> Option<Role> {
        match self {
            Self::Cmaf(track) => track.kind().role(),
            Self::TimedText(track) => track.format().text().role,
            Self::Thumbnail(_) => None,
        }
    }

    pub fn cmaf(&self) -> Option<&ResolvedCmafTrack> {
        match self {
            Self::Cmaf(track) => Some(track),
            Self::TimedText(_) | Self::Thumbnail(_) => None,
        }
    }

    pub fn timed_text(&self) -> Option<&ResolvedTimedTextTrack> {
        match self {
            Self::TimedText(track) => Some(track),
            Self::Cmaf(_) | Self::Thumbnail(_) => None,
        }
    }

    pub fn thumbnail(&self) -> Option<&ResolvedThumbnailTrack> {
        match self {
            Self::Thumbnail(track) => Some(track),
            Self::Cmaf(_) | Self::TimedText(_) => None,
        }
    }

    /// Returns the stored source path, or `None` for derived representations.
    pub fn source_path(&self) -> Option<&RelativePath> {
        match self {
            Self::Cmaf(track) => track.source_path(),
            Self::TimedText(track) => Some(track.source_path()),
            Self::Thumbnail(_) => None,
        }
    }

    /// Builds the CMAF representation supported by this track.
    pub async fn cmaf_representation(
        &self,
        options: &SegmentOptions,
    ) -> Result<ResolvedCmafTrack, CmafRepresentationError> {
        match self {
            Self::Cmaf(track) => Ok(track.clone()),
            Self::TimedText(track) => track.package_wvtt(options).await.map_err(Into::into),
            Self::Thumbnail(_) => Err(CmafRepresentationError::UnsupportedTrack),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TrackResolveError {
    #[error(transparent)]
    Cmaf(#[from] CmafError),
    #[error(transparent)]
    TimedText(#[from] TimedTextError),
}

#[derive(Debug, thiserror::Error)]
pub enum CmafRepresentationError {
    #[error(transparent)]
    WebVtt(#[from] WebVttPackageError),
    #[error("track has no CMAF representation")]
    UnsupportedTrack,
}

fn source_id(path: &RelativePath) -> String {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, path.as_str().as_bytes()).to_string()
}
