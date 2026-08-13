use language_tags::LanguageTag;
use opendal::Operator;
use relative_path::RelativePath;
use uuid::Uuid;

use super::cmaf::{CmafError, CmafMetadata, ResolvedCmafTrack};
use super::metadata::{AudioMetadata, TextMetadata, VideoMetadata};
use super::thumbnail::ResolvedThumbnailTrack;
use super::timed_text::{ResolvedTimedTextTrack, TimedTextError, WebVttPackageError};
use super::{TrackFormat, TrackType};
use crate::role::Role;

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

    /// Returns the playback category of this track.
    pub fn track_type(&self) -> TrackType {
        match self {
            Self::Cmaf(track) => track.metadata().track_type(),
            Self::TimedText(_) => TrackType::Text,
            Self::Thumbnail(_) => TrackType::Thumbnail,
        }
    }

    /// Returns the stored or generated form of this track.
    pub fn format(&self) -> TrackFormat {
        match self {
            Self::Cmaf(_) => TrackFormat::Cmaf,
            Self::TimedText(track) => track.format().track_format(),
            Self::Thumbnail(_) => TrackFormat::Thumbnail,
        }
    }

    pub fn codec(&self) -> Option<String> {
        match self {
            Self::Cmaf(track) => Some(track.codec().rfc6381()),
            Self::TimedText(_) | Self::Thumbnail(_) => None,
        }
    }

    pub fn video_metadata(&self) -> Option<&VideoMetadata> {
        match self {
            Self::Cmaf(track) => match track.metadata() {
                CmafMetadata::Video(metadata) => Some(metadata),
                CmafMetadata::Audio(_) | CmafMetadata::Text(_) => None,
            },
            Self::TimedText(_) | Self::Thumbnail(_) => None,
        }
    }

    pub fn audio_metadata(&self) -> Option<&AudioMetadata> {
        match self {
            Self::Cmaf(track) => match track.metadata() {
                CmafMetadata::Audio(metadata) => Some(metadata),
                CmafMetadata::Video(_) | CmafMetadata::Text(_) => None,
            },
            Self::TimedText(_) | Self::Thumbnail(_) => None,
        }
    }

    pub fn text_metadata(&self) -> Option<&TextMetadata> {
        match self {
            Self::Cmaf(track) => match track.metadata() {
                CmafMetadata::Text(metadata) => Some(metadata),
                CmafMetadata::Video(_) | CmafMetadata::Audio(_) => None,
            },
            Self::TimedText(track) => Some(track.metadata()),
            Self::Thumbnail(_) => None,
        }
    }

    pub fn language(&self) -> Option<&LanguageTag> {
        match self {
            Self::Cmaf(track) => track.metadata().language(),
            Self::TimedText(track) => Some(&track.metadata().language),
            Self::Thumbnail(_) => None,
        }
    }

    pub fn role(&self) -> Option<Role> {
        match self {
            Self::Cmaf(track) => track.metadata().role(),
            Self::TimedText(track) => track.metadata().role,
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
        text_length: u32,
        boundaries: &[u32],
    ) -> Result<ResolvedCmafTrack, CmafRepresentationError> {
        match self {
            Self::Cmaf(track) => Ok(track.clone()),
            Self::TimedText(track) => track
                .package_wvtt(text_length, boundaries)
                .await
                .map_err(Into::into),
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
