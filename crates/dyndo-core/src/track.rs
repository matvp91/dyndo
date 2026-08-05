use std::ops::Range;

use bytes::Bytes;
use opendal::Operator;
use relative_path::{RelativePath, RelativePathBuf};
use uuid::Uuid;

use crate::asset_descriptor::TrackKind;
use crate::track_probe::{self, TrackProbeError};
use crate::track_source::{TrackSource, TrackSourceError};

#[derive(Debug, thiserror::Error)]
pub enum TrackError {
    #[error(transparent)]
    Probe(#[from] TrackProbeError),
    #[error(transparent)]
    Source(#[from] TrackSourceError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fragment {
    pub(crate) byte_offset: u64,
    pub(crate) byte_size: u64,
    pub(crate) duration: u64,
}

impl Fragment {
    pub(crate) fn new(byte_offset: u64, byte_size: u64, duration: u64) -> Option<Self> {
        byte_offset.checked_add(byte_size)?;
        Some(Self {
            byte_offset,
            byte_size,
            duration,
        })
    }

    pub(crate) fn byte_range(&self) -> Range<u64> {
        self.byte_offset..self.byte_offset + self.byte_size
    }

    pub(crate) fn duration(&self) -> u64 {
        self.duration
    }
}

pub struct Track {
    id: Uuid,
    path: RelativePathBuf,
    codec: String,
    kind: TrackKind,
    timescale: u32,
    earliest_presentation_time: u64,
    initialization_range: Range<u64>,
    fragments: Vec<Fragment>,
    source: TrackSource,
}

impl Track {
    pub async fn probe(
        op: &Operator,
        path: &RelativePath,
        kind: Option<TrackKind>,
    ) -> Result<Self, TrackError> {
        let probed = track_probe::probe(op, path).await?;
        let kind = kind.unwrap_or(probed.kind);
        let id = Uuid::new_v5(&Uuid::NAMESPACE_URL, path.as_str().as_bytes());

        Ok(Self {
            id,
            path: path.to_owned(),
            codec: probed.codec,
            kind,
            timescale: probed.timescale,
            earliest_presentation_time: probed.earliest_presentation_time,
            initialization_range: probed.initialization_range,
            fragments: probed.fragments,
            source: probed.source,
        })
    }

    pub fn id(&self) -> String {
        format!("{}_{}", self.content_type(), self.id)
    }

    pub fn path(&self) -> &RelativePath {
        &self.path
    }

    pub fn kind(&self) -> &TrackKind {
        &self.kind
    }

    /// Returns the DASH media content type represented by the track.
    pub fn content_type(&self) -> &'static str {
        match self.kind {
            TrackKind::Video(_) => "video",
            TrackKind::Audio(_) => "audio",
            TrackKind::Text(_) => "text",
        }
    }

    /// Returns the media type of the track's CMAF representation.
    pub fn mime_type(&self) -> &'static str {
        match self.kind {
            TrackKind::Video(_) => "video/mp4",
            TrackKind::Audio(_) => "audio/mp4",
            TrackKind::Text(_) => "application/mp4",
        }
    }

    pub fn codec(&self) -> &str {
        &self.codec
    }

    pub fn timescale(&self) -> u32 {
        self.timescale
    }

    /// Returns the track's earliest presentation time in timescale units.
    pub fn earliest_presentation_time(&self) -> u64 {
        self.earliest_presentation_time
    }

    /// Returns the byte range containing the track's CMAF initialization segment.
    pub fn initialization_range(&self) -> Range<u64> {
        self.initialization_range.clone()
    }

    pub(crate) fn fragments(&self) -> &[Fragment] {
        &self.fragments
    }

    /// Returns the total duration of the track's fragments in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        let duration_units: u128 = self
            .fragments
            .iter()
            .map(|reference| u128::from(reference.duration()))
            .sum();
        let duration_ms = duration_units * 1000 / u128::from(self.timescale);
        u64::try_from(duration_ms).unwrap_or(u64::MAX)
    }

    pub async fn read_range(&self, op: &Operator, range: Range<u64>) -> Result<Bytes, TrackError> {
        Ok(self.source.read_range(op, &self.path, range).await?)
    }

    /// Reads the track's CMAF initialization segment.
    pub async fn read_initialization(&self, op: &Operator) -> Result<Bytes, TrackError> {
        self.read_range(op, self.initialization_range()).await
    }
}
