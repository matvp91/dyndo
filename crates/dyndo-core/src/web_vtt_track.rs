use bytes::Bytes;
use relative_path::{RelativePath, RelativePathBuf};

use super::cmaf_package::CmafPackage;
use super::cmaf_track::CmafTrack;
use super::cmaf_track_kind::{CmafTrackKind, TextKind};
use super::packaging::PackageError;
use super::probe::ProbeError;
use super::segment_options::SegmentOptions;
use super::text::{Cue, Subtitle};

#[derive(Debug, thiserror::Error)]
pub enum WebVttPackageError {
    #[error(transparent)]
    Package(#[from] PackageError),
    #[error(transparent)]
    Cmaf(#[from] ProbeError),
}

/// A resolved raw WebVTT track.
#[derive(Clone)]
pub struct WebVttTrack {
    id: String,
    path: RelativePathBuf,
    kind: TextKind,
    subtitle: Subtitle,
}

impl WebVttTrack {
    pub(crate) fn new(
        id: String,
        path: RelativePathBuf,
        kind: TextKind,
        subtitle: Subtitle,
    ) -> Self {
        Self {
            id,
            path,
            kind,
            subtitle,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn path(&self) -> &RelativePath {
        &self.path
    }

    pub fn kind(&self) -> &TextKind {
        &self.kind
    }

    pub(crate) fn package_bytes(&self, options: &SegmentOptions) -> Result<Bytes, PackageError> {
        self.subtitle
            .to_wvtt(options.text_length, &options.boundaries)
            .map(Bytes::from)
    }

    /// Packages this source as temporary CMAF media.
    pub async fn package(
        &self,
        options: &SegmentOptions,
    ) -> Result<CmafPackage, WebVttPackageError> {
        let bytes = self.package_bytes(options)?;
        let cmaf = CmafTrack::from_bytes(
            bytes.clone(),
            self.path(),
            self.id().to_string(),
            CmafTrackKind::Text(self.kind().clone()),
        )
        .await?;
        Ok(CmafPackage::new(cmaf, bytes))
    }

    /// Returns the raw WebVTT document for a served segment.
    pub fn vtt_segment(&self, start: u64, end: u64) -> Option<String> {
        let start = u32::try_from(start).ok()?;
        let end = u32::try_from(end).ok()?;
        if start >= end {
            return None;
        }
        let cues = self
            .subtitle
            .cues
            .iter()
            .filter(|cue| cue.start < end && cue.end > start)
            .map(|cue| Cue {
                start: cue.start.max(start),
                end: cue.end.min(end),
                text: cue.text.clone(),
            })
            .collect();

        Some(Subtitle { cues }.to_vtt_text())
    }
}
