use std::ops::Range;
use std::sync::Arc;

use super::codec::CodecConfig;
use super::time::Time;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    init_segment: Arc<InitSegment>,
    unscaled_start_time: u64,
    unscaled_end_time: u64,
    start_byte: u64,
    end_byte: u64,
}

impl Segment {
    pub fn new(
        init_segment: Arc<InitSegment>,
        unscaled_start_time: u64,
        unscaled_end_time: u64,
        start_byte: u64,
        end_byte: u64,
    ) -> Self {
        Self {
            init_segment,
            unscaled_start_time,
            unscaled_end_time,
            start_byte,
            end_byte,
        }
    }

    pub fn init_segment(&self) -> &InitSegment {
        &self.init_segment
    }

    pub fn unscaled_start_time(&self) -> u64 {
        self.unscaled_start_time
    }

    pub fn unscaled_end_time(&self) -> u64 {
        self.unscaled_end_time
    }

    pub fn start_time(&self) -> u64 {
        Time::milliseconds(self.unscaled_start_time, self.init_segment.timescale())
    }

    pub fn end_time(&self) -> u64 {
        Time::milliseconds(self.unscaled_end_time, self.init_segment.timescale())
    }

    pub fn byte_range(&self) -> Range<u64> {
        self.start_byte..self.end_byte
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct InitSegment {
    codec: CodecConfig,
    timescale: u32,
    start_byte: u64,
    end_byte: u64,
}

impl InitSegment {
    pub fn new(codec: CodecConfig, timescale: u32, start_byte: u64, end_byte: u64) -> Self {
        Self {
            codec,
            timescale,
            start_byte,
            end_byte,
        }
    }

    pub fn codec(&self) -> &CodecConfig {
        &self.codec
    }

    pub fn timescale(&self) -> u32 {
        self.timescale
    }

    pub fn byte_range(&self) -> Range<u64> {
        self.start_byte..self.end_byte
    }
}
