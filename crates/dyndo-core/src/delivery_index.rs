use std::{sync::Arc, time::Duration};

use crate::{
    segment::{InitSegment, Segment},
    media_index::MediaIndex,
};

const STRICT_TIMESCALE: u32 = 1_000;

/// Addressable media segments derived from a source segment index.
pub struct DeliveryIndex {
    init_segment: Arc<InitSegment>,
    segments: Vec<Segment>,
}

impl DeliveryIndex {
    /// Creates strict, zero-byte segments using a millisecond timeline.
    ///
    /// Each segment is no longer than `target_duration`, and each boundary is
    /// an exact segment boundary. Boundaries outside `duration` and duplicate
    /// boundaries are ignored.
    ///
    /// # Panics
    ///
    /// Panics when `target_duration` is less than one millisecond.
    pub fn strict(duration: Duration, target_duration: Duration, boundaries: &[Duration]) -> Self {
        let duration = duration.as_millis() as u64;
        let target_duration = target_duration.as_millis() as u64;
        assert!(target_duration > 0);

        let init_segment = Arc::new(InitSegment::new(0..0, STRICT_TIMESCALE));
        let mut boundaries: Vec<_> = boundaries
            .iter()
            .copied()
            .map(|boundary| Duration::from_millis(boundary.as_millis() as u64))
            .filter(|boundary| {
                *boundary > Duration::ZERO && boundary.as_millis() < u128::from(duration)
            })
            .collect();
        boundaries.push(Duration::from_millis(duration));
        boundaries.sort_unstable();
        boundaries.dedup();

        let mut source_segments = Vec::new();
        let mut start = 0;
        for &boundary in &boundaries {
            let boundary = boundary.as_millis() as u64;
            while start < boundary {
                let end = start.saturating_add(target_duration).min(boundary);
                source_segments.push(Segment::new(Arc::clone(&init_segment), start, end, 0..0));
                start = end;
            }
        }

        Self::source_aligned(
            init_segment,
            &source_segments,
            Duration::from_millis(target_duration),
            &boundaries,
        )
    }

    /// Groups source segments until a delivery segment reaches `min_duration`
    /// or reaches a splice boundary.
    pub fn source_aligned(
        init_segment: Arc<InitSegment>,
        source_segments: &[Segment],
        min_duration: Duration,
        boundaries: &[Duration],
    ) -> Self {
        let cuts = boundary_cuts(source_segments, boundaries);
        let mut segments = Vec::new();
        let mut start = 0;
        let mut next_cut = 0;

        while start < source_segments.len() {
            while next_cut < cuts.len() && cuts[next_cut] <= start {
                next_cut += 1;
            }

            let mut end = start + 1;
            while end < source_segments.len()
                && !has_reached_duration(source_segments, start, end, min_duration)
                && (next_cut == cuts.len() || end != cuts[next_cut])
            {
                end += 1;
            }

            segments.push(source_segments[start].combined(&source_segments[end - 1]));
            start = end;
        }

        Self {
            init_segment,
            segments,
        }
    }

    /// Returns the native timeline tick for DASH's `presentationTimeOffset`.
    pub fn presentation_time_offset(&self) -> Option<u64> {
        self.segments.first().map(Segment::start_ticks)
    }

    /// Returns the number of addressable media segments.
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Returns whether there are no addressable media segments.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Returns the addressable media segment at `index`.
    pub fn get(&self, index: usize) -> Option<&Segment> {
        self.segments.get(index)
    }

    /// Iterates over addressable media segments in presentation order.
    pub fn iter(&self) -> impl Iterator<Item = &Segment> {
        self.segments.iter()
    }

    /// Finds the addressable media segment at the supplied presentation time.
    pub fn find(&self, start_time: Duration) -> Option<&Segment> {
        let index = self
            .segments
            .partition_point(|segment| segment.start_time() < start_time);
        let segment = self.get(index)?;

        (segment.start_time() == start_time).then_some(segment)
    }
}

impl MediaIndex for DeliveryIndex {
    fn init_segment(&self) -> &InitSegment {
        &self.init_segment
    }

    fn segments(&self) -> &[Segment] {
        &self.segments
    }
}

fn boundary_cuts(segments: &[Segment], boundaries: &[Duration]) -> Vec<usize> {
    let mut cuts: Vec<_> = boundaries
        .iter()
        .map(|&boundary| segments.partition_point(|segment| segment.start_time() < boundary))
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    cuts
}

fn has_reached_duration(
    segments: &[Segment],
    start: usize,
    end: usize,
    min_duration: Duration,
) -> bool {
    segments[end - 1]
        .end_time()
        .saturating_sub(segments[start].start_time())
        >= min_duration
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use super::{DeliveryIndex, Segment};
    use crate::{
        media_index::MediaIndex, segment::InitSegment, segment_index::SegmentIndex,
    };

    fn index(specifications: &[(u64, u64, u64, u64)]) -> SegmentIndex {
        index_with_timescale(specifications, 1_000)
    }

    fn index_with_timescale(
        specifications: &[(u64, u64, u64, u64)],
        timescale: u32,
    ) -> SegmentIndex {
        let init_segment = Arc::new(InitSegment::new(0..100, timescale));
        let segments = specifications
            .iter()
            .map(|&(start, end, start_byte, end_byte)| {
                Segment::new(Arc::clone(&init_segment), start, end, start_byte..end_byte)
            })
            .collect();

        SegmentIndex::new(init_segment, segments)
    }

    fn duration(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    fn source_aligned(
        source: &SegmentIndex,
        min_duration: Duration,
        boundaries: &[Duration],
    ) -> DeliveryIndex {
        DeliveryIndex::source_aligned(
            Arc::clone(source.init_segment()),
            source.segments(),
            min_duration,
            boundaries,
        )
    }

    fn tick_durations(index: &DeliveryIndex) -> Vec<u64> {
        index.iter().map(Segment::duration_ticks).collect()
    }

    #[test]
    fn source_aligned_returns_no_segments_when_the_source_is_empty() {
        let index = index(&[]);
        let delivery_index = source_aligned(&index, duration(1_000), &[]);

        assert!(delivery_index.is_empty());
    }

    #[test]
    fn source_aligned_keeps_the_initialization_range_when_the_source_is_empty() {
        let index = index(&[]);
        let delivery_index = source_aligned(&index, duration(1_000), &[]);

        assert_eq!(delivery_index.init_range(), 0..100);
    }

    #[test]
    fn source_aligned_keeps_the_timescale_when_the_source_is_empty() {
        let index = index_with_timescale(&[], 90_000);
        let delivery_index = source_aligned(&index, duration(1_000), &[]);

        assert_eq!(delivery_index.timescale(), 90_000);
    }

    #[test]
    fn source_aligned_keeps_each_source_segment_when_minimum_duration_is_zero() {
        let index = index(&[(0, 500, 0, 50), (500, 1_000, 50, 100)]);
        let delivery_index = source_aligned(&index, Duration::ZERO, &[]);

        assert_eq!(tick_durations(&delivery_index), [500, 500]);
    }

    #[test]
    fn source_aligned_accumulates_source_segments_until_the_minimum_duration() {
        let index = index(&[(0, 400, 0, 40), (400, 800, 40, 80), (800, 1_200, 80, 120)]);
        let delivery_index = source_aligned(&index, duration(800), &[]);

        assert_eq!(tick_durations(&delivery_index), [800, 400]);
    }

    #[test]
    fn source_aligned_compares_minimum_duration_using_the_source_timescale() {
        let index = index_with_timescale(&[(0, 45_000, 0, 40), (45_000, 90_000, 40, 80)], 90_000);
        let delivery_index = source_aligned(&index, duration(1_000), &[]);

        assert_eq!(tick_durations(&delivery_index), [90_000]);
    }

    #[test]
    fn source_aligned_cuts_at_the_first_source_segment_starting_at_a_boundary() {
        let index = index(&[
            (0, 500, 0, 50),
            (500, 1_000, 50, 100),
            (1_000, 1_500, 100, 150),
        ]);
        let delivery_index = source_aligned(&index, duration(1_500), &[duration(1_000)]);

        assert_eq!(tick_durations(&delivery_index), [1_000, 500]);
    }

    #[test]
    fn source_aligned_normalizes_unsorted_duplicate_boundaries() {
        let index = index(&[
            (0, 500, 0, 50),
            (500, 1_000, 50, 100),
            (1_000, 1_500, 100, 150),
        ]);
        let delivery_index = source_aligned(
            &index,
            duration(1_500),
            &[duration(1_000), duration(1_000), duration(500)],
        );

        assert_eq!(tick_durations(&delivery_index), [500, 500, 500]);
    }

    #[test]
    fn delivery_segments_remain_available_after_the_source_index_is_dropped() {
        let delivery_index = {
            let index = index(&[(0, 500, 12, 32), (500, 1_000, 32, 80)]);

            source_aligned(&index, duration(1_000), &[])
        };

        assert_eq!(delivery_index.get(0).unwrap().byte_range(), 12..80);
    }

    #[test]
    fn find_returns_the_segment_at_its_start_time() {
        let index = index(&[(0, 500, 0, 50), (500, 1_000, 50, 100)]);
        let delivery_index = source_aligned(&index, duration(500), &[]);

        assert_eq!(
            delivery_index.find(duration(500)).unwrap().byte_range(),
            50..100
        );
    }

    #[test]
    fn max_bitrate_returns_the_largest_delivery_segment_bitrate() {
        let index = index(&[(0, 1_000, 0, 100), (1_000, 2_000, 100, 300)]);
        let delivery_index = source_aligned(&index, Duration::ZERO, &[]);

        assert_eq!(delivery_index.max_bitrate(), 1_600);
    }

    #[test]
    fn avg_bitrate_weights_delivery_segments_by_duration() {
        let index = index(&[(0, 1_000, 0, 100), (1_000, 3_000, 100, 500)]);
        let delivery_index = source_aligned(&index, Duration::ZERO, &[]);

        assert_eq!(delivery_index.avg_bitrate(), 1_334);
    }

    #[test]
    fn strict_creates_target_duration_windows_and_honors_boundaries() {
        let index = DeliveryIndex::strict(duration(16_000), duration(6_000), &[duration(9_000)]);
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
    fn strict_uses_milliseconds_and_empty_byte_ranges() {
        let index = DeliveryIndex::strict(duration(1_000), duration(1_000), &[]);

        assert_eq!(
            (
                index.timescale(),
                index.init_range(),
                index.segments()[0].byte_range(),
            ),
            (1_000, 0..0, 0..0)
        );
    }

    #[test]
    fn strict_ignores_duplicate_and_out_of_range_boundaries() {
        let index = DeliveryIndex::strict(
            duration(16_000),
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
