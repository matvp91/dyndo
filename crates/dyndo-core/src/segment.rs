use std::{ops::Range, sync::Arc, time::Duration};

/// The initialization section shared by all source segments in an index.
#[derive(Debug, PartialEq, Eq)]
pub struct InitSegment {
    byte_range: Range<u64>,
    timescale: u32,
}

impl InitSegment {
    /// Creates initialization context with the source header range and media timescale.
    ///
    /// # Panics
    ///
    /// Panics when `timescale` is zero.
    pub fn new(byte_range: Range<u64>, timescale: u32) -> Self {
        assert!(timescale > 0);

        Self {
            byte_range,
            timescale,
        }
    }

    /// Returns the source byte range containing the initialization section.
    pub fn byte_range(&self) -> Range<u64> {
        self.byte_range.clone()
    }

    /// Returns the number of media timeline ticks per second.
    pub fn timescale(&self) -> u32 {
        self.timescale
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    init_segment: Arc<InitSegment>,
    start_ticks: u64,
    end_ticks: u64,
    byte_range: Range<u64>,
}

impl Segment {
    /// Creates a source segment associated with its initialization context.
    pub fn new(
        init_segment: Arc<InitSegment>,
        start_ticks: u64,
        end_ticks: u64,
        byte_range: Range<u64>,
    ) -> Self {
        Self {
            init_segment,
            start_ticks,
            end_ticks,
            byte_range,
        }
    }

    /// Returns the initialization context shared with sibling source segments.
    pub fn init_segment(&self) -> &InitSegment {
        &self.init_segment
    }

    /// Returns the native timeline tick at which this segment starts.
    pub fn start_ticks(&self) -> u64 {
        self.start_ticks
    }

    /// Returns the presentation time at which this segment starts.
    pub fn start_time(&self) -> Duration {
        self.presentation_time(self.start_ticks)
    }

    /// Returns the native timeline tick at which this segment ends.
    pub fn end_ticks(&self) -> u64 {
        self.end_ticks
    }

    /// Returns the presentation time at which this segment ends.
    pub fn end_time(&self) -> Duration {
        self.presentation_time(self.end_ticks)
    }

    /// Returns this segment's duration in native timeline ticks.
    pub fn duration_ticks(&self) -> u64 {
        self.end_ticks.saturating_sub(self.start_ticks)
    }

    /// Returns this segment's presentation duration.
    pub fn duration_time(&self) -> Duration {
        self.presentation_time(self.duration_ticks())
    }

    /// Returns the source byte range containing this segment.
    pub fn byte_range(&self) -> Range<u64> {
        self.byte_range.clone()
    }

    /// Returns the size of this segment in bytes.
    pub fn byte_size(&self) -> u64 {
        let range = self.byte_range();
        range.end.saturating_sub(range.start)
    }

    /// Returns this segment's bitrate in bits per second.
    pub fn bitrate(&self) -> u64 {
        let duration = self.duration_ticks();
        if duration == 0 {
            return 0;
        }

        let bits = u128::from(self.byte_size()) * 8;
        let scaled_bits = bits * u128::from(self.init_segment.timescale());
        u64::try_from(scaled_bits.div_ceil(u128::from(duration))).unwrap_or(u64::MAX)
    }

    /// Returns a segment spanning this segment through `last`.
    pub fn combined(&self, last: &Self) -> Self {
        Self {
            init_segment: Arc::clone(&self.init_segment),
            start_ticks: self.start_ticks,
            end_ticks: last.end_ticks,
            byte_range: self.byte_range.start..last.byte_range.end,
        }
    }

    fn presentation_time(&self, timestamp: u64) -> Duration {
        let timescale = u64::from(self.init_segment.timescale());

        Duration::from_secs(timestamp / timescale)
            + Duration::from_nanos(timestamp % timescale * 1_000_000_000 / timescale)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{InitSegment, Segment};

    #[test]
    fn duration_ticks_is_the_difference_between_end_and_start() {
        let init_segment = Arc::new(InitSegment::new(0..100, 1_000));
        let segment = Segment::new(init_segment, 500, 1_500, 100..200);

        assert_eq!(segment.duration_ticks(), 1_000);
    }

    #[test]
    fn byte_size_is_the_difference_between_byte_range_bounds() {
        let init_segment = Arc::new(InitSegment::new(0..100, 1_000));
        let segment = Segment::new(init_segment, 0, 1_000, 100..350);

        assert_eq!(segment.byte_size(), 250);
    }

    #[test]
    fn bitrate_uses_the_segment_timescale() {
        let init_segment = Arc::new(InitSegment::new(0..100, 1_000));
        let segment = Segment::new(init_segment, 0, 1_000, 0..100);

        assert_eq!(segment.bitrate(), 800);
    }

    #[test]
    fn combined_spans_its_first_and_last_segments() {
        let init_segment = Arc::new(InitSegment::new(0..100, 1_000));
        let first = Segment::new(Arc::clone(&init_segment), 0, 500, 100..150);
        let last = Segment::new(init_segment, 500, 1_000, 150..225);

        assert_eq!(first.combined(&last).byte_range(), 100..225);
    }
}
