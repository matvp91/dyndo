use std::ops::Range;

use super::segment::Segment;

/// One addressable media segment after consecutive source segments are grouped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServedSegment<'a> {
    segments: &'a [Segment],
}

impl<'a> ServedSegment<'a> {
    pub fn group(segments: &'a [Segment], minimum_duration: u32, boundaries: &[u32]) -> Vec<Self> {
        if segments.is_empty() {
            return Vec::new();
        }

        let ranges = if minimum_duration == 0 {
            (0..segments.len()).map(|index| index..index + 1).collect()
        } else {
            minimum_ranges(segments, minimum_duration, boundaries)
        };

        ranges
            .into_iter()
            .map(|range| Self {
                segments: &segments[range],
            })
            .collect()
    }

    pub fn segments(&self) -> &'a [Segment] {
        self.segments
    }

    pub fn byte_range(&self) -> Range<u64> {
        self.first().byte_range().start..self.last().byte_range().end
    }

    pub fn byte_size(&self) -> u64 {
        let range = self.byte_range();
        range.end.saturating_sub(range.start)
    }

    pub fn unscaled_start_time(&self) -> u64 {
        self.first().unscaled_start_time()
    }

    pub fn unscaled_end_time(&self) -> u64 {
        self.last().unscaled_end_time()
    }

    pub fn unscaled_duration(&self) -> u64 {
        self.unscaled_end_time()
            .saturating_sub(self.unscaled_start_time())
    }

    pub fn start_time(&self) -> u64 {
        self.first().start_time()
    }

    pub fn end_time(&self) -> u64 {
        self.last().end_time()
    }

    pub fn duration(&self) -> u64 {
        self.end_time().saturating_sub(self.start_time())
    }

    pub fn bitrate(&self) -> u64 {
        let duration = self.unscaled_duration();
        if duration == 0 {
            return 0;
        }

        let bits = u128::from(self.byte_size()) * 8;
        let scaled_bits = bits * u128::from(self.timescale());
        u64::try_from(scaled_bits.div_ceil(u128::from(duration))).unwrap_or(u64::MAX)
    }

    pub fn maximum_bitrate(segments: &[Self]) -> u64 {
        segments.iter().map(Self::bitrate).max().unwrap_or(0)
    }

    /// Returns total bits divided by total duration for segments from one track,
    /// not the mean of their individual bitrates.
    pub fn average_bitrate(segments: &[Self]) -> u64 {
        let Some(first) = segments.first() else {
            return 0;
        };
        let (bytes, duration) = segments.iter().fold((0_u128, 0_u128), |total, segment| {
            (
                total.0 + u128::from(segment.byte_size()),
                total.1 + u128::from(segment.unscaled_duration()),
            )
        });
        if duration == 0 {
            return 0;
        }

        let bits = bytes * 8;
        let scaled_bits = bits * u128::from(first.timescale());
        u64::try_from(scaled_bits.div_ceil(duration)).unwrap_or(u64::MAX)
    }

    fn first(&self) -> &Segment {
        // `group` only constructs served segments from non-empty ranges.
        &self.segments[0]
    }

    fn last(&self) -> &Segment {
        // `group` only constructs served segments from non-empty ranges.
        &self.segments[self.segments.len() - 1]
    }

    fn timescale(&self) -> u32 {
        self.first().init_segment().timescale()
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
