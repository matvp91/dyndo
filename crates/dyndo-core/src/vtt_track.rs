use bytes::Bytes;
use relative_path::{RelativePath, RelativePathBuf};

use super::cmaf_track_kind::TextKind;
use super::packaging::PackageError;
use super::segment_options::SegmentOptions;
use super::text::{Cue, Subtitle};

/// A resolved raw WebVTT track.
#[derive(Clone)]
pub struct VttTrack {
    id: String,
    path: RelativePathBuf,
    kind: TextKind,
    subtitle: Subtitle,
}

impl VttTrack {
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

    /// Returns the raw VTT document for a served segment.
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
