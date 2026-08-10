use std::sync::Arc;

use super::super::codec::CodecConfig;
use super::super::segment::{InitSegment, Segment};
use super::ProbeError;
use super::box_reader::Boxes;

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
) -> Result<Vec<Segment>, ProbeError> {
    let mut start_byte = boxes
        .sidx_end
        .checked_add(boxes.sidx.first_offset)
        .ok_or(ProbeError::SegmentOffsetOverflow)?;
    let mut unscaled_start_time = boxes.sidx.earliest_presentation_time;
    let mut segments = Vec::with_capacity(boxes.sidx.references.len());

    for reference in &boxes.sidx.references {
        let end_byte = start_byte
            .checked_add(u64::from(reference.reference_size))
            .ok_or(ProbeError::SegmentRangeOverflow)?;
        let unscaled_end_time = unscaled_start_time
            .checked_add(u64::from(reference.subsegment_duration))
            .ok_or(ProbeError::SegmentTimeOverflow)?;

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
