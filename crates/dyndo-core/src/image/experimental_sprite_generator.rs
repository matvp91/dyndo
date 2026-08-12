//! Experimental, single-pass JPEG sprite generation.

use std::ops::Range;

use bytes::Bytes;
use opendal::Operator;

use crate::track::cmaf::{CmafReadError, ResolvedCmafTrack};

/// Selects the source frame for a sprite tile.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameSelection {
    /// Return the displayed frame at the requested time.
    #[default]
    Exact,
    /// Return the random-access frame at the start of the containing CMAF segment.
    PreviousKeyframe,
}

/// A deliberately small proof of concept for generating one JPEG sprite.
pub(crate) struct ExperimentalSpriteGenerator<'a> {
    op: &'a Operator,
    track: &'a ResolvedCmafTrack,
}

impl<'a> ExperimentalSpriteGenerator<'a> {
    pub(crate) fn new(op: &'a Operator, track: &'a ResolvedCmafTrack) -> Self {
        Self { op, track }
    }

    /// Plans the contiguous CMAF media range required for `times`.
    ///
    /// The eventual decoder consumes this range once, after the initialization segment.
    pub(crate) fn media_range(&self, times: &[u64]) -> Option<Range<u64>> {
        let first = *times.first()?;
        let last = *times.last()?;
        let segments = self.track.segments();
        let first = segments
            .iter()
            .find(|segment| segment.start_time() <= first && first < segment.end_time())?;
        let last = segments
            .iter()
            .find(|segment| segment.start_time() <= last && last < segment.end_time())?;
        Some(first.byte_range().start..last.byte_range().end)
    }

    /// Resolves the source timestamps used by the requested selection policy.
    pub(crate) fn source_times(
        &self,
        times: &[u64],
        selection: FrameSelection,
    ) -> Option<Vec<u64>> {
        times
            .iter()
            .map(|&time| match selection {
                FrameSelection::Exact => Some(time),
                FrameSelection::PreviousKeyframe => self
                    .track
                    .segments()
                    .iter()
                    .find(|segment| segment.start_time() <= time && time < segment.end_time())
                    .map(|segment| segment.start_time()),
            })
            .collect()
    }

    /// Reads the init segment and one contiguous media window.
    ///
    /// This is intentionally the only storage-facing operation in the proof of concept. The next
    /// iteration replaces `media` with a bounded byte stream feeding one FFmpeg decoder.
    pub(crate) async fn input(&self, times: &[u64]) -> Result<SpriteInput, CmafReadError> {
        let media_range = self.media_range(times).ok_or(CmafReadError::Range)?;
        let (initialization, media) = tokio::try_join!(
            self.track
                .read_range(self.op, self.track.init_segment().byte_range()),
            self.track.read_range(self.op, media_range),
        )?;
        Ok(SpriteInput {
            initialization,
            media,
        })
    }
}

/// The CMAF bytes a single sprite decoder receives.
pub(crate) struct SpriteInput {
    pub(crate) initialization: Bytes,
    pub(crate) media: Bytes,
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use opendal::{Operator, services::Memory};
    use relative_path::RelativePath;

    use super::{ExperimentalSpriteGenerator, FrameSelection};
    use crate::track::ResolvedTrack;

    const FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/two-segment-black-white-h264.mp4");

    #[tokio::test]
    async fn input_reads_one_range_for_two_segments() {
        let op = Operator::new(Memory::default()).unwrap();
        let path = RelativePath::new("video.mp4");
        op.write(path.as_str(), Bytes::from_static(FIXTURE))
            .await
            .unwrap();
        let track = ResolvedTrack::discover(&op, path).await.unwrap();
        let generator = ExperimentalSpriteGenerator::new(&op, track.cmaf().unwrap());

        let input = generator.input(&[0, 500]).await.unwrap();

        assert!(!input.media.is_empty());
    }

    #[tokio::test]
    async fn previous_keyframe_uses_the_segment_start() {
        let op = Operator::new(Memory::default()).unwrap();
        let path = RelativePath::new("video.mp4");
        op.write(path.as_str(), Bytes::from_static(FIXTURE))
            .await
            .unwrap();
        let track = ResolvedTrack::discover(&op, path).await.unwrap();
        let generator = ExperimentalSpriteGenerator::new(&op, track.cmaf().unwrap());

        let times = generator
            .source_times(&[499, 500], FrameSelection::PreviousKeyframe)
            .unwrap();

        assert_eq!(times, [0, 500]);
    }
}
