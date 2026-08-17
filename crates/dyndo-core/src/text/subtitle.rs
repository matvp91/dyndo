use std::{sync::Arc, time::Duration};

use crate::{
    segment::{InitSegment, Segment},
    segment_index::SegmentIndex,
};

const TIMESCALE: u32 = 1_000;

/// A timed-text cue with presentation timestamps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    /// The inclusive start time.
    pub start: Duration,
    /// The exclusive end time.
    pub end: Duration,
    /// The cue content.
    pub text: String,
}

/// A collection of timed-text cues.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Subtitle {
    /// The subtitle cues.
    pub cues: Vec<Cue>,
}

impl Subtitle {
    /// Returns the timestamp of the latest cue end.
    pub fn duration(&self) -> Duration {
        self.cues
            .iter()
            .map(|cue| cue.end)
            .max()
            .unwrap_or_default()
    }

    /// Creates a delivery segment index for this subtitle.
    ///
    /// The index uses a millisecond timescale. It divides the subtitle into
    /// windows no longer than `target_duration`, with each supplied boundary
    /// becoming an exact segment boundary. Boundaries outside the subtitle
    /// duration and duplicate boundaries are ignored.
    ///
    /// # Panics
    ///
    /// Panics when `target_duration` is less than one millisecond.
    pub fn segment_index(
        &self,
        target_duration: Duration,
        boundaries: &[Duration],
    ) -> SegmentIndex {
        let duration = self.duration().as_millis() as u64;
        let target_duration = target_duration.as_millis() as u64;
        assert!(target_duration > 0);

        let init_segment = Arc::new(InitSegment::new(0..0, TIMESCALE));
        let mut boundaries: Vec<_> = boundaries
            .iter()
            .copied()
            .map(|boundary| boundary.as_millis() as u64)
            .filter(|&boundary| boundary > 0 && boundary < duration)
            .collect();
        boundaries.push(duration);
        boundaries.sort_unstable();
        boundaries.dedup();

        let mut segments = Vec::new();
        let mut start = 0;
        for boundary in boundaries {
            while start < boundary {
                let end = start.saturating_add(target_duration).min(boundary);
                segments.push(Segment::new(Arc::clone(&init_segment), start, end, 0..0));
                start = end;
            }
        }

        SegmentIndex::new(init_segment, segments)
    }

    /// Returns the portion of this subtitle that overlaps `start..end`.
    pub fn slice(&self, start: Duration, end: Duration) -> Option<Self> {
        if start >= end {
            return None;
        }

        let cues = self
            .cues
            .iter()
            .filter(|cue| cue.start < end && cue.end > start)
            .map(|cue| Cue {
                start: cue.start.max(start),
                end: cue.end.min(end),
                text: cue.text.clone(),
            })
            .collect();

        Some(Self { cues })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Cue, Subtitle};
    use crate::segment_timeline::SegmentTimeline;

    fn duration(milliseconds: u64) -> Duration {
        Duration::from_millis(milliseconds)
    }

    #[test]
    fn slice_clamps_overlapping_cues_to_its_range() {
        let subtitle = Subtitle {
            cues: vec![Cue {
                start: duration(500),
                end: duration(1_500),
                text: "cue".into(),
            }],
        };

        assert_eq!(
            subtitle.slice(duration(1_000), duration(2_000)),
            Some(Subtitle {
                cues: vec![Cue {
                    start: duration(1_000),
                    end: duration(1_500),
                    text: "cue".into(),
                }],
            })
        );
    }

    #[test]
    fn segment_index_creates_target_duration_windows_and_honors_boundaries() {
        let subtitle = Subtitle {
            cues: vec![Cue {
                start: Duration::ZERO,
                end: duration(16_000),
                text: "cue".into(),
            }],
        };

        let index = subtitle.segment_index(duration(6_000), &[duration(9_000)]);
        let ranges: Vec<_> = index
            .segments()
            .iter()
            .map(|segment| (segment.start_ticks(), segment.end_ticks()))
            .collect();

        assert_eq!(
            ranges,
            [
                (0, 6_000),
                (6_000, 9_000),
                (9_000, 15_000),
                (15_000, 16_000)
            ]
        );
    }

    #[test]
    fn segment_index_uses_milliseconds_and_empty_byte_ranges() {
        let subtitle = Subtitle {
            cues: vec![Cue {
                start: Duration::ZERO,
                end: duration(1_000),
                text: "cue".into(),
            }],
        };

        let index = subtitle.segment_index(duration(1_000), &[]);

        assert_eq!(
            (
                index.init_segment().timescale(),
                index.init_segment().byte_range(),
                index.segments()[0].byte_range(),
            ),
            (1_000, 0..0, 0..0)
        );
    }

    #[test]
    fn segment_index_ignores_duplicate_and_out_of_range_boundaries() {
        let subtitle = Subtitle {
            cues: vec![Cue {
                start: Duration::ZERO,
                end: duration(16_000),
                text: "cue".into(),
            }],
        };

        let index = subtitle.segment_index(
            duration(10_000),
            &[
                Duration::ZERO,
                duration(6_000),
                duration(6_000),
                duration(20_000),
            ],
        );
        let ranges: Vec<_> = index
            .segments()
            .iter()
            .map(|segment| (segment.start_ticks(), segment.end_ticks()))
            .collect();

        assert_eq!(ranges, [(0, 6_000), (6_000, 16_000)]);
    }
}
