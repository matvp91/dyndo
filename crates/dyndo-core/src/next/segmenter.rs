use std::ops::Range;
use std::sync::Arc;

use super::segment::Segment;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segmenter {
    minimum_duration: u32,
    boundaries: Arc<[u32]>,
}

impl Segmenter {
    pub fn new(minimum_duration: u32, boundaries: &[u32]) -> Self {
        Self {
            minimum_duration,
            boundaries: boundaries.into(),
        }
    }

    pub fn group(&self, segments: &[Segment]) -> Vec<Range<usize>> {
        if segments.is_empty() {
            return Vec::new();
        }

        if self.minimum_duration == 0 {
            return (0..segments.len()).map(|index| index..index + 1).collect();
        }

        minimum_ranges(segments, self.minimum_duration, &self.boundaries)
    }
}

fn minimum_ranges(segments: &[Segment], minimum: u32, boundaries: &[u32]) -> Vec<Range<usize>> {
    let cuts = snapped_cuts(segments, boundaries.iter().copied());
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut next_cut = 0;

    for end in 1..=segments.len() {
        while next_cut < cuts.len() && cuts[next_cut] <= start {
            next_cut += 1;
        }
        let duration = segments[end - 1]
            .end_time()
            .saturating_sub(segments[start].start_time());
        let long_enough = duration >= u64::from(minimum);
        let at_cut = next_cut < cuts.len() && cuts[next_cut] == end;
        if long_enough || at_cut || end == segments.len() {
            ranges.push(start..end);
            start = end;
        }
    }

    ranges
}

fn snapped_cuts(segments: &[Segment], boundaries: impl IntoIterator<Item = u32>) -> Vec<usize> {
    let mut cuts: Vec<_> = boundaries
        .into_iter()
        .map(|boundary| {
            segments.partition_point(|segment| segment.start_time() < u64::from(boundary))
        })
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    cuts
}
