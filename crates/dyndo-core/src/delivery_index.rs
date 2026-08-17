use std::{ops::Range, time::Duration};

use crate::{segment::Segment, segment_index::SegmentIndex};

/// Addressable media segments derived from a source segment index.
pub struct DeliveryIndex {
    init_range: Range<u64>,
    timescale: u32,
    avg_bitrate: u64,
    segments: Vec<Segment>,
}

impl DeliveryIndex {
    /// Groups source segments until a delivery segment reaches `min_duration`
    /// or reaches a splice boundary.
    pub fn new(source: &SegmentIndex, min_duration: Duration, boundaries: &[Duration]) -> Self {
        let init_segment = source.init_segment();
        let init_range = init_segment.byte_range();
        let timescale = init_segment.timescale();
        let avg_bitrate = source.avg_bitrate();
        let source_segments = source.segments();
        if source_segments.is_empty() {
            return Self {
                init_range,
                timescale,
                avg_bitrate,
                segments: Vec::new(),
            };
        }
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
            init_range,
            timescale,
            avg_bitrate,
            segments,
        }
    }

    /// Returns the source byte range containing the initialization section.
    pub fn init_range(&self) -> Range<u64> {
        self.init_range.clone()
    }

    /// Returns the number of native media timeline ticks per second.
    pub fn timescale(&self) -> u32 {
        self.timescale
    }

    /// Returns the native timeline tick for DASH's `presentationTimeOffset`.
    pub fn presentation_time_offset(&self) -> Option<u64> {
        self.segments.first().map(Segment::start_ticks)
    }

    /// Returns the addressable media segments in presentation order.
    pub fn segments(&self) -> &[Segment] {
        &self.segments
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

    /// Returns the highest bitrate among the addressable media segments.
    pub fn max_bitrate(&self) -> u64 {
        self.iter().map(Segment::bitrate).max().unwrap_or(0)
    }

    /// Returns the source media's average bitrate.
    pub fn avg_bitrate(&self) -> u64 {
        self.avg_bitrate
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

    use super::{DeliveryIndex, Segment, SegmentIndex};
    use crate::segment::InitSegment;

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

        SegmentIndex::for_test(init_segment, segments)
    }

    fn duration(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    fn tick_durations(index: &DeliveryIndex) -> Vec<u64> {
        index.iter().map(Segment::duration_ticks).collect()
    }

    #[test]
    fn new_returns_no_segments_when_the_source_is_empty() {
        let index = index(&[]);
        let delivery_index = DeliveryIndex::new(&index, duration(1_000), &[]);

        assert!(delivery_index.is_empty());
    }

    #[test]
    fn new_keeps_the_initialization_range_when_the_source_is_empty() {
        let index = index(&[]);
        let delivery_index = DeliveryIndex::new(&index, duration(1_000), &[]);

        assert_eq!(delivery_index.init_range(), 0..100);
    }

    #[test]
    fn new_keeps_the_timescale_when_the_source_is_empty() {
        let index = index_with_timescale(&[], 90_000);
        let delivery_index = DeliveryIndex::new(&index, duration(1_000), &[]);

        assert_eq!(delivery_index.timescale(), 90_000);
    }

    #[test]
    fn new_keeps_each_source_segment_when_minimum_duration_is_zero() {
        let index = index(&[(0, 500, 0, 50), (500, 1_000, 50, 100)]);
        let delivery_index = DeliveryIndex::new(&index, Duration::ZERO, &[]);

        assert_eq!(tick_durations(&delivery_index), [500, 500]);
    }

    #[test]
    fn new_accumulates_source_segments_until_the_minimum_duration() {
        let index = index(&[(0, 400, 0, 40), (400, 800, 40, 80), (800, 1_200, 80, 120)]);
        let delivery_index = DeliveryIndex::new(&index, duration(800), &[]);

        assert_eq!(tick_durations(&delivery_index), [800, 400]);
    }

    #[test]
    fn new_compares_minimum_duration_using_the_source_timescale() {
        let index = index_with_timescale(&[(0, 45_000, 0, 40), (45_000, 90_000, 40, 80)], 90_000);
        let delivery_index = DeliveryIndex::new(&index, duration(1_000), &[]);

        assert_eq!(tick_durations(&delivery_index), [90_000]);
    }

    #[test]
    fn new_cuts_at_the_first_source_segment_starting_at_a_boundary() {
        let index = index(&[
            (0, 500, 0, 50),
            (500, 1_000, 50, 100),
            (1_000, 1_500, 100, 150),
        ]);
        let delivery_index = DeliveryIndex::new(&index, duration(1_500), &[duration(1_000)]);

        assert_eq!(tick_durations(&delivery_index), [1_000, 500]);
    }

    #[test]
    fn new_normalizes_unsorted_duplicate_boundaries() {
        let index = index(&[
            (0, 500, 0, 50),
            (500, 1_000, 50, 100),
            (1_000, 1_500, 100, 150),
        ]);
        let delivery_index = DeliveryIndex::new(
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

            DeliveryIndex::new(&index, duration(1_000), &[])
        };

        assert_eq!(delivery_index.get(0).unwrap().byte_range(), 12..80);
    }

    #[test]
    fn find_returns_the_segment_at_its_start_time() {
        let index = index(&[(0, 500, 0, 50), (500, 1_000, 50, 100)]);
        let delivery_index = DeliveryIndex::new(&index, duration(500), &[]);

        assert_eq!(
            delivery_index.find(duration(500)).unwrap().byte_range(),
            50..100
        );
    }

    #[test]
    fn max_bitrate_returns_the_largest_delivery_segment_bitrate() {
        let index = index(&[(0, 1_000, 0, 100), (1_000, 2_000, 100, 300)]);
        let delivery_index = DeliveryIndex::new(&index, Duration::ZERO, &[]);

        assert_eq!(delivery_index.max_bitrate(), 1_600);
    }

    #[test]
    fn avg_bitrate_weights_delivery_segments_by_duration() {
        let index = index(&[(0, 1_000, 0, 100), (1_000, 3_000, 100, 500)]);
        let delivery_index = DeliveryIndex::new(&index, Duration::ZERO, &[]);

        assert_eq!(delivery_index.avg_bitrate(), 1_334);
    }
}
