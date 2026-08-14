use std::ops::Range;
use std::sync::Arc;

use super::CmafError;
use super::boxes::Boxes;
use crate::codec::CodecConfig;
use crate::time::Time;

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

#[derive(Debug, Clone, PartialEq, Eq)]
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

pub(super) fn build_init_segment(boxes: &Boxes, codec: CodecConfig) -> Arc<InitSegment> {
    Arc::new(InitSegment::new(
        codec,
        boxes.sidx.timescale,
        0,
        boxes.moov_end,
    ))
}

pub(super) fn build_segments(
    boxes: &Boxes,
    init_segment: &Arc<InitSegment>,
) -> Result<Vec<Segment>, CmafError> {
    let mut start_byte = boxes
        .sidx_end
        .checked_add(boxes.sidx.first_offset)
        .ok_or(CmafError::SegmentOffsetOverflow)?;
    let mut unscaled_start_time = boxes.sidx.earliest_presentation_time;
    let mut segments = Vec::with_capacity(boxes.sidx.references.len());

    for reference in &boxes.sidx.references {
        let end_byte = start_byte
            .checked_add(u64::from(reference.reference_size))
            .ok_or(CmafError::SegmentRangeOverflow)?;
        let unscaled_end_time = unscaled_start_time
            .checked_add(u64::from(reference.subsegment_duration))
            .ok_or(CmafError::SegmentTimeOverflow)?;

        segments.push(Segment::new(
            Arc::clone(init_segment),
            unscaled_start_time,
            unscaled_end_time,
            start_byte,
            end_byte,
        ));

        start_byte = end_byte;
        unscaled_start_time = unscaled_end_time;
    }

    Ok(segments)
}
