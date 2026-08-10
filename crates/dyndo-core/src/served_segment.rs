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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{Segment, ServedSegment};
    use crate::codec::{CodecConfig, WvttCodec};
    use crate::segment::InitSegment;

    fn segments(specifications: &[(u64, u64, u64, u64)]) -> Vec<Segment> {
        let init = Arc::new(InitSegment::new(CodecConfig::Wvtt(WvttCodec), 1_000, 0, 0));
        specifications
            .iter()
            .map(|&(start, end, start_byte, end_byte)| {
                Segment::new(Arc::clone(&init), start, end, start_byte, end_byte)
            })
            .collect()
    }

    #[test]
    fn group_returns_no_served_segments_when_the_source_is_empty() {
        assert!(ServedSegment::group(&[], 1_000, &[]).is_empty());
    }

    #[test]
    fn group_keeps_each_source_segment_when_minimum_duration_is_zero() {
        let segments = segments(&[(0, 500, 0, 50), (500, 1_000, 50, 100)]);

        let grouped = ServedSegment::group(&segments, 0, &[]);

        assert_eq!(
            grouped
                .iter()
                .map(|segment| segment.segments().len())
                .collect::<Vec<_>>(),
            [1, 1]
        );
    }

    #[test]
    fn group_accumulates_source_segments_until_the_minimum_duration() {
        let segments = segments(&[(0, 400, 0, 40), (400, 800, 40, 80), (800, 1_200, 80, 120)]);

        let grouped = ServedSegment::group(&segments, 800, &[]);

        assert_eq!(
            grouped
                .iter()
                .map(|segment| segment.segments().len())
                .collect::<Vec<_>>(),
            [2, 1]
        );
    }

    #[test]
    fn group_cuts_at_a_boundary_before_the_minimum_duration() {
        let segments = segments(&[
            (0, 500, 0, 50),
            (500, 1_000, 50, 100),
            (1_000, 1_500, 100, 150),
        ]);

        let grouped = ServedSegment::group(&segments, 1_500, &[1_000]);

        assert_eq!(
            grouped
                .iter()
                .map(|segment| segment.segments().len())
                .collect::<Vec<_>>(),
            [2, 1]
        );
    }

    #[test]
    fn group_normalizes_unsorted_duplicate_boundaries() {
        let segments = segments(&[
            (0, 500, 0, 50),
            (500, 1_000, 50, 100),
            (1_000, 1_500, 100, 150),
        ]);

        let grouped = ServedSegment::group(&segments, 1_500, &[1_000, 1_000, 500]);

        assert_eq!(
            grouped
                .iter()
                .map(|segment| segment.segments().len())
                .collect::<Vec<_>>(),
            [1, 1, 1]
        );
    }

    #[test]
    fn average_bitrate_weights_segments_by_their_duration() {
        let segments = segments(&[(0, 1_000, 0, 100), (1_000, 3_000, 100, 500)]);
        let grouped = ServedSegment::group(&segments, 0, &[]);

        assert_eq!(ServedSegment::average_bitrate(&grouped), 1_334);
    }

    #[test]
    fn bitrate_is_zero_when_a_segment_has_no_duration() {
        let segments = segments(&[(0, 0, 0, 100)]);
        let grouped = ServedSegment::group(&segments, 0, &[]);

        assert_eq!(grouped[0].bitrate(), 0);
    }
}
