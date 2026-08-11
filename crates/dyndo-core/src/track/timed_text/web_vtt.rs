use bytes::Bytes;

use crate::packaging::PackageError;
use crate::probe::ProbeError;
use crate::segment_options::SegmentOptions;
use crate::text::{Cue, Subtitle};
use crate::track::cmaf::CmafTrack;
use crate::track::cmaf::package::CmafPackage;
use crate::track::kind::CmafTrackKind;
use crate::track::timed_text::TimedTextTrack;

#[derive(Debug, thiserror::Error)]
pub enum TimedTextPackageError {
    #[error(transparent)]
    Package(#[from] PackageError),
    #[error(transparent)]
    Cmaf(#[from] ProbeError),
}

impl TimedTextTrack {
    pub(crate) fn package_bytes(&self, options: &SegmentOptions) -> Result<Bytes, PackageError> {
        self.subtitle
            .to_wvtt(options.text_length, &options.boundaries)
            .map(Bytes::from)
    }

    /// Packages this source as temporary CMAF media.
    pub async fn package_wvtt(
        &self,
        options: &SegmentOptions,
    ) -> Result<CmafPackage, TimedTextPackageError> {
        let bytes = self.package_bytes(options)?;
        let cmaf = CmafTrack::from_bytes(
            bytes.clone(),
            self.path(),
            self.id().to_string(),
            CmafTrackKind::Text(self.kind().text().clone()),
        )
        .await?;
        Ok(CmafPackage::new(cmaf, bytes))
    }

    /// Returns the raw WebVTT document for a served segment.
    pub fn web_vtt_segment(&self, start: u64, end: u64) -> Option<String> {
        if !self.kind().is_web_vtt() {
            return None;
        }
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
