use opendal::Operator;
use relative_path::{RelativePath, RelativePathBuf};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{TrackFormat, metadata::TextMetadata};
use crate::text::{Subtitle, WebVttParseError};

mod web_vtt;

pub use web_vtt::WebVttPackageError;

/// A timed-text track stored in an asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TimedTextTrack {
    pub id: String,
    /// Path relative to the asset file.
    #[schemars(with = "String")]
    pub(super) path: RelativePathBuf,
    #[serde(flatten)]
    pub format: TimedTextFormat,
}

impl TimedTextTrack {
    /// Resolves this configured timed-text source.
    pub async fn resolve(
        &self,
        op: &Operator,
        path: &RelativePath,
    ) -> Result<ResolvedTimedTextTrack, TimedTextError> {
        match &self.format {
            TimedTextFormat::WebVtt(metadata) => {
                ResolvedTimedTextTrack::from_web_vtt_source(
                    op,
                    path,
                    self.id.clone(),
                    metadata.clone(),
                )
                .await
            }
        }
    }
}

/// The document format carried by a timed-text source track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TimedTextFormat {
    WebVtt(TextMetadata),
}

impl TimedTextFormat {
    pub fn text(&self) -> &TextMetadata {
        match self {
            Self::WebVtt(metadata) => metadata,
        }
    }

    pub fn text_mut(&mut self) -> &mut TextMetadata {
        match self {
            Self::WebVtt(metadata) => metadata,
        }
    }

    /// Returns the serialized source discriminator used in `asset.json`.
    pub const fn asset_type(&self) -> &'static str {
        match self {
            Self::WebVtt(_) => "webvtt",
        }
    }

    /// Returns the format used to expose this timed-text source as a track.
    pub const fn track_format(&self) -> TrackFormat {
        match self {
            Self::WebVtt(_) => TrackFormat::WebVtt,
        }
    }

    pub fn is_web_vtt(&self) -> bool {
        matches!(self, Self::WebVtt(_))
    }
}

/// A source track represented by a parsed timed-text document.
#[derive(Clone)]
pub struct ResolvedTimedTextTrack {
    id: String,
    source_path: RelativePathBuf,
    format: TimedTextFormat,
    subtitle: Subtitle,
}

impl ResolvedTimedTextTrack {
    fn new(
        id: String,
        source_path: RelativePathBuf,
        format: TimedTextFormat,
        subtitle: Subtitle,
    ) -> Self {
        Self {
            id,
            source_path,
            format,
            subtitle,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn source_path(&self) -> &RelativePath {
        &self.source_path
    }

    pub fn format(&self) -> &TimedTextFormat {
        &self.format
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TimedTextError {
    #[error(transparent)]
    Storage(#[from] opendal::Error),
    #[error("timed-text source is not UTF-8")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    WebVtt(#[from] WebVttParseError),
}
