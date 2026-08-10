use std::iter::successors;
use std::ops::Range;

use super::super::time::Time;
use super::{DurationPolicy, SegmentationPolicy, partition};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segmenter {
    policy: SegmentationPolicy,
}

impl Segmenter {
    pub fn new(policy: SegmentationPolicy) -> Self {
        Self { policy }
    }

    pub fn policy(&self) -> &SegmentationPolicy {
        &self.policy
    }

    /// Produces exact media ranges for sources that dyndo can cut at any time.
    pub fn exact(&self, duration: u32) -> Vec<Range<u32>> {
        let divisions = self
            .policy
            .boundaries()
            .iter()
            .copied()
            .chain(grid(self.policy.duration().duration(), duration));
        partition(0..duration, divisions)
    }

    pub fn constrained(&self, durations: &[u32], timescale: u32) -> Vec<Range<usize>> {
        if durations.is_empty() || timescale == 0 {
            return Vec::new();
        }

        let mut cumulative = Vec::with_capacity(durations.len() + 1);
        cumulative.push(0u64);
        for &duration in durations {
            cumulative.push(cumulative[cumulative.len() - 1].saturating_add(u64::from(duration)));
        }

        match self.policy.duration() {
            DurationPolicy::Exact(length) => {
                let duration = Time::milliseconds(*cumulative.last().unwrap_or(&0), timescale);
                let duration = u32::try_from(duration).unwrap_or(u32::MAX);
                let boundaries = self
                    .policy
                    .boundaries()
                    .iter()
                    .copied()
                    .chain(grid(length, duration));
                let cuts = snapped_cuts(&cumulative, timescale, boundaries);
                index_ranges(durations.len(), &cuts)
            }
            DurationPolicy::Minimum(0) => {
                (0..durations.len()).map(|index| index..index + 1).collect()
            }
            DurationPolicy::Minimum(minimum) => {
                minimum_ranges(&cumulative, timescale, minimum, self.policy.boundaries())
            }
        }
    }
}

fn minimum_ranges(
    cumulative: &[u64],
    timescale: u32,
    minimum: u32,
    boundaries: &[u32],
) -> Vec<Range<usize>> {
    let cuts = snapped_cuts(cumulative, timescale, boundaries.iter().copied());
    let minimum = u128::from(minimum) * u128::from(timescale);
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut next_cut = 0;

    for end in 1..cumulative.len() {
        while next_cut < cuts.len() && cuts[next_cut] <= start {
            next_cut += 1;
        }
        let duration = cumulative[end] - cumulative[start];
        let long_enough = u128::from(duration) * 1_000 >= minimum;
        let at_cut = next_cut < cuts.len() && cuts[next_cut] == end;
        if long_enough || at_cut || end + 1 == cumulative.len() {
            ranges.push(start..end);
            start = end;
        }
    }

    ranges
}

fn snapped_cuts(
    cumulative: &[u64],
    timescale: u32,
    boundaries: impl IntoIterator<Item = u32>,
) -> Vec<usize> {
    let mut cuts: Vec<_> = boundaries
        .into_iter()
        .map(|boundary| {
            let target = u128::from(boundary) * u128::from(timescale);
            cumulative
                .partition_point(|&duration| u128::from(duration) * 1_000 < target)
                .min(cumulative.len() - 1)
        })
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    cuts
}

fn index_ranges(end: usize, cuts: &[usize]) -> Vec<Range<usize>> {
    partition(0..end, cuts.iter().copied())
}

fn grid(length: u32, end: u32) -> impl Iterator<Item = u32> {
    successors((length > 0).then_some(length), move |time| {
        time.checked_add(length)
    })
    .take_while(move |&time| time < end)
}
