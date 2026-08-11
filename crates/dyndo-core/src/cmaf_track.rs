use std::sync::Arc;

use relative_path::{RelativePath, RelativePathBuf};

use super::cmaf_track_kind::CmafTrackKind;
use super::codec::CodecConfig;
use super::segment::{InitSegment, Segment};

#[derive(Clone)]
pub struct CmafTrack {
    id: String,
    path: RelativePathBuf,
    kind: CmafTrackKind,
    init_segment: Arc<InitSegment>,
    segments: Vec<Segment>,
}

impl CmafTrack {
    pub fn new(
        id: String,
        path: RelativePathBuf,
        kind: CmafTrackKind,
        init_segment: Arc<InitSegment>,
        segments: Vec<Segment>,
    ) -> Self {
        Self {
            id,
            path,
            kind,
            init_segment,
            segments,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn path(&self) -> &RelativePath {
        &self.path
    }

    pub fn kind(&self) -> &CmafTrackKind {
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
}
