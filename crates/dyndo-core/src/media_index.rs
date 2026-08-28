use std::ops::Range;

use crate::segment::{InitSegment, Segment};

/// An ordered index of addressable media segments.
pub trait MediaIndex {
    /// Returns the initialization context shared by the media segments.
    fn init_segment(&self) -> &InitSegment;

    /// Returns the media segments in presentation order.
    fn segments(&self) -> &[Segment];

    /// Returns the source byte range containing the initialization section.
    fn init_range(&self) -> Range<u64> {
        self.init_segment().byte_range()
    }

    /// Returns the number of native media timeline ticks per second.
    fn timescale(&self) -> u32 {
        self.init_segment().timescale()
    }

    /// Returns total media bits divided by total media duration.
    fn avg_bitrate(&self) -> u64 {
        let (bytes, duration) =
            self.segments()
                .iter()
                .fold((0_u128, 0_u128), |(bytes, duration), segment| {
                    (
                        bytes + u128::from(segment.byte_size()),
                        duration + u128::from(segment.duration_ticks()),
                    )
                });

        if duration == 0 {
            return 0;
        }

        let bits = bytes * 8;
        let scaled_bits = bits * u128::from(self.timescale());
        u64::try_from(scaled_bits.div_ceil(duration)).unwrap_or(u64::MAX)
    }

    /// Returns the highest bitrate among the media segments.
    fn max_bitrate(&self) -> u64 {
        self.segments()
            .iter()
            .map(Segment::bitrate)
            .max()
            .unwrap_or(0)
    }
}
