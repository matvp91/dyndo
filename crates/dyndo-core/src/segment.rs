use std::ops::Range;

use crate::media_time::MediaTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    start_time: MediaTime,
    end_time: MediaTime,
    byte_range: Range<u64>,
}

impl Segment {
    pub fn new(
        unscaled_start_time: u64,
        unscaled_end_time: u64,
        timescale: u32,
        byte_range: Range<u64>,
    ) -> Self {
        Self {
            start_time: MediaTime::new(unscaled_start_time, timescale),
            end_time: MediaTime::new(unscaled_end_time, timescale),
            byte_range,
        }
    }

    pub fn start(&self) -> MediaTime {
        self.start_time
    }

    pub fn end(&self) -> MediaTime {
        self.end_time
    }

    pub fn byte_range(&self) -> Range<u64> {
        self.byte_range.clone()
    }
}
