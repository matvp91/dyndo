use std::sync::Arc;

use mp4_atom::{Moov, Sidx};

use crate::{
    mp4_box_reader::Mp4BoxReader,
    mp4_readable::{Mp4Readable, Mp4ReadableError},
    segment::{InitSegment, Segment},
    media_index::MediaIndex,
};

/// The initialization section and source media segments read from a CMAF index.
pub struct SegmentIndex {
    init_segment: Arc<InitSegment>,
    segments: Vec<Segment>,
}

impl SegmentIndex {
    /// Creates an index from an initialization context and source segments.
    pub fn new(init_segment: Arc<InitSegment>, segments: Vec<Segment>) -> Self {
        Self {
            init_segment,
            segments,
        }
    }

    /// Returns the initialization context shared by all source segments.
    pub fn init_segment(&self) -> &Arc<InitSegment> {
        &self.init_segment
    }
}

impl MediaIndex for SegmentIndex {
    fn init_segment(&self) -> &InitSegment {
        &self.init_segment
    }

    fn segments(&self) -> &[Segment] {
        &self.segments
    }
}

impl Mp4Readable for SegmentIndex {
    type Error = Mp4ReadableError;
    type Output = Self;

    async fn from_mp4_reader(
        reader: &mut Mp4BoxReader<impl tokio::io::AsyncRead + Unpin>,
    ) -> Result<Self, Self::Error> {
        let _: Moov = reader.read_box().await?;
        let init_range = 0..reader.position();
        let sidx = reader.read_box::<Sidx>().await?;
        let sidx_end_offset = reader.position();

        if sidx.timescale == 0 {
            return Err(Mp4ReadableError::invalid("timescale cannot be zero"));
        }
        let init_segment = Arc::new(InitSegment::new(init_range, sidx.timescale));

        Ok(Self::new(
            Arc::clone(&init_segment),
            parse_sidx_references(&sidx, sidx_end_offset, init_segment)?,
        ))
    }
}

fn parse_sidx_references(
    sidx: &Sidx,
    sidx_end_offset: u64,
    init_segment: Arc<InitSegment>,
) -> Result<Vec<Segment>, Mp4ReadableError> {
    let mut start_ticks = sidx.earliest_presentation_time;
    let mut start_byte = sidx_end_offset
        .checked_add(sidx.first_offset)
        .ok_or_else(|| Mp4ReadableError::invalid("first offset overflows the byte range"))?;
    let mut segments = Vec::with_capacity(sidx.references.len());

    for (index, reference) in sidx.references.iter().enumerate() {
        let end_ticks = start_ticks
            .checked_add(u64::from(reference.subsegment_duration))
            .ok_or_else(|| {
                Mp4ReadableError::invalid(format!("segment {index} time range overflows"))
            })?;
        let end_byte = start_byte
            .checked_add(u64::from(reference.reference_size))
            .ok_or_else(|| {
                Mp4ReadableError::invalid(format!("segment {index} byte range overflows"))
            })?;

        segments.push(Segment::new(
            Arc::clone(&init_segment),
            start_ticks,
            end_ticks,
            start_byte..end_byte,
        ));

        start_ticks = end_ticks;
        start_byte = end_byte;
    }

    Ok(segments)
}
