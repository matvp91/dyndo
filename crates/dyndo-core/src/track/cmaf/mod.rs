use std::ops::Range;
use std::sync::Arc;

use bytes::Bytes;
use language_tags::LanguageTag;
use opendal::Operator;
use relative_path::{RelativePath, RelativePathBuf};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::codec::CodecConfig;
use crate::role::Role;
use crate::track::TrackType;
use crate::track::metadata::{AudioMetadata, TextMetadata, VideoMetadata};

mod boxes;
mod inspect;
mod segments;
mod served;

pub use boxes::CmafBoxesError;
pub use segments::{InitSegment, Segment};
pub use served::ServedSegment;

/// A CMAF track stored in an asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CmafTrack {
    pub id: String,
    /// Path relative to the asset file.
    #[schemars(with = "String")]
    pub(super) path: RelativePathBuf,
    pub codec: String,
    #[serde(flatten)]
    pub kind: CmafKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CmafKind {
    Video(VideoMetadata),
    Audio(AudioMetadata),
    Text(TextMetadata),
}

impl CmafKind {
    /// Returns the playback category of this CMAF track.
    pub const fn track_type(&self) -> TrackType {
        match self {
            Self::Video(_) => TrackType::Video,
            Self::Audio(_) => TrackType::Audio,
            Self::Text(_) => TrackType::Text,
        }
    }

    pub fn content_type(&self) -> &'static str {
        self.track_type().as_str()
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Video(_) => "video/mp4",
            Self::Audio(_) => "audio/mp4",
            Self::Text(_) => "application/mp4",
        }
    }

    pub fn video(&self) -> Option<&VideoMetadata> {
        match self {
            Self::Video(metadata) => Some(metadata),
            Self::Audio(_) | Self::Text(_) => None,
        }
    }

    pub fn audio(&self) -> Option<&AudioMetadata> {
        match self {
            Self::Video(_) | Self::Text(_) => None,
            Self::Audio(metadata) => Some(metadata),
        }
    }

    pub fn language(&self) -> Option<&LanguageTag> {
        match self {
            Self::Video(_) => None,
            Self::Audio(metadata) => Some(&metadata.language),
            Self::Text(metadata) => Some(&metadata.language),
        }
    }

    pub fn role(&self) -> Option<Role> {
        match self {
            Self::Video(_) => None,
            Self::Audio(metadata) => metadata.role,
            Self::Text(metadata) => metadata.role,
        }
    }

    pub fn language_and_role_mut(&mut self) -> Option<(&mut LanguageTag, &mut Option<Role>)> {
        match self {
            Self::Video(_) => None,
            Self::Audio(metadata) => Some((&mut metadata.language, &mut metadata.role)),
            Self::Text(metadata) => Some((&mut metadata.language, &mut metadata.role)),
        }
    }
}

#[derive(Clone)]
enum CmafBacking {
    Stored { path: RelativePathBuf },
    Memory { bytes: Bytes },
}

/// Parsed and indexed CMAF media backed by storage or memory.
#[derive(Clone)]
pub struct ResolvedCmafTrack {
    id: String,
    backing: CmafBacking,
    kind: CmafKind,
    init_segment: Arc<InitSegment>,
    segments: Vec<Segment>,
}

impl ResolvedCmafTrack {
    pub fn new(
        id: String,
        source_path: RelativePathBuf,
        kind: CmafKind,
        init_segment: Arc<InitSegment>,
        segments: Vec<Segment>,
    ) -> Self {
        Self {
            id,
            backing: CmafBacking::Stored { path: source_path },
            kind,
            init_segment,
            segments,
        }
    }

    fn from_memory(
        id: String,
        bytes: Bytes,
        kind: CmafKind,
        init_segment: Arc<InitSegment>,
        segments: Vec<Segment>,
    ) -> Self {
        Self {
            id,
            backing: CmafBacking::Memory { bytes },
            kind,
            init_segment,
            segments,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the source path when this representation is backed by storage.
    pub fn source_path(&self) -> Option<&RelativePath> {
        match &self.backing {
            CmafBacking::Stored { path } => Some(path),
            CmafBacking::Memory { .. } => None,
        }
    }

    pub fn kind(&self) -> &CmafKind {
        &self.kind
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    pub fn init_segment(&self) -> &InitSegment {
        &self.init_segment
    }

    pub fn codec(&self) -> &CodecConfig {
        self.init_segment().codec()
    }

    pub fn timescale(&self) -> u32 {
        self.init_segment().timescale()
    }

    pub fn unscaled_earliest_presentation_time(&self) -> Option<u64> {
        self.segments.first().map(Segment::unscaled_start_time)
    }

    pub fn duration(&self) -> u32 {
        let Some((first, remaining)) = self.segments.split_first() else {
            return 0;
        };
        let last = remaining.last().unwrap_or(first);
        let duration = last.end_time().saturating_sub(first.start_time());
        u32::try_from(duration).unwrap_or(u32::MAX)
    }

    /// Reads a byte range from the stored source or in-memory package.
    pub async fn read_range(
        &self,
        op: &Operator,
        range: Range<u64>,
    ) -> Result<Bytes, CmafReadError> {
        match &self.backing {
            CmafBacking::Stored { path } => {
                Ok(op.read_with(path.as_str()).range(range).await?.to_bytes())
            }
            CmafBacking::Memory { bytes } => {
                let start = usize::try_from(range.start).map_err(|_| CmafReadError::Range)?;
                let end = usize::try_from(range.end).map_err(|_| CmafReadError::Range)?;
                if start > end || end > bytes.len() {
                    return Err(CmafReadError::Range);
                }
                Ok(bytes.slice(start..end))
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CmafReadError {
    #[error(transparent)]
    Storage(#[from] opendal::Error),
    #[error("invalid CMAF byte range")]
    Range,
}

#[derive(Debug, thiserror::Error)]
pub enum CmafError {
    #[error(transparent)]
    Boxes(#[from] CmafBoxesError),
    #[error("unsupported video sample entry")]
    UnsupportedVideoSampleEntry,
    #[error("unsupported audio sample entry")]
    UnsupportedAudioSampleEntry,
    #[error("unsupported track handler")]
    UnsupportedTrackHandler,
    #[error("video track has no sample duration")]
    MissingFrameRate,
    #[error("unsupported codec {0}")]
    UnsupportedCodec(String),
    #[error("segment offset overflows")]
    SegmentOffsetOverflow,
    #[error("segment range overflows")]
    SegmentRangeOverflow,
    #[error("segment time overflows")]
    SegmentTimeOverflow,
}
