use std::{ops::Range, sync::Arc};

use futures_util::io::AsyncRead;
use mp4_atom::{Moov, Sidx};
use thiserror::Error;
use tokio_util::compat::FuturesAsyncReadCompatExt;

use crate::{
    mp4_box_reader::Mp4BoxReader,
    mp4_readable::Mp4Readable,
    segment::{InitSegment, Segment},
};

/// The initialization section and source media segments read from a CMAF index.
pub struct SegmentIndex {
    init_segment: Arc<InitSegment>,
    segments: Vec<Segment>,
}

#[derive(Debug, Error)]
pub enum SegmentIndexError {
    #[error("failed to read MP4: {0}")]
    Mp4(#[from] mp4_atom::Error),
    #[error("invalid segment index: {0}")]
    InvalidSidx(String),
}

impl SegmentIndex {
    pub(crate) fn from_sidx(
        init_range: Range<u64>,
        sidx: Sidx,
        sidx_end_offset: u64,
    ) -> Result<Self, SegmentIndexError> {
        if sidx.timescale == 0 {
            return Err(SegmentIndexError::InvalidSidx(
                "timescale cannot be zero".into(),
            ));
        }
        let init_segment = Arc::new(InitSegment::new(init_range, sidx.timescale));

        Ok(Self {
            init_segment: Arc::clone(&init_segment),
            segments: parse_sidx_references(&sidx, sidx_end_offset, init_segment)?,
        })
    }

    /// Returns the initialization context shared by all source segments.
    pub fn init_segment(&self) -> &InitSegment {
        &self.init_segment
    }

    /// Returns the source media segments in presentation order.
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// Returns total media bits divided by total media duration.
    pub fn avg_bitrate(&self) -> u64 {
        let (bytes, duration) =
            self.segments
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
        let scaled_bits = bits * u128::from(self.init_segment.timescale());
        u64::try_from(scaled_bits.div_ceil(duration)).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
impl SegmentIndex {
    pub(crate) fn for_test(init_segment: Arc<InitSegment>, segments: Vec<Segment>) -> Self {
        Self {
            init_segment,
            segments,
        }
    }
}

impl Mp4Readable for SegmentIndex {
    type Error = SegmentIndexError;

    async fn from_reader(reader: &mut (impl AsyncRead + Unpin)) -> Result<Self, Self::Error> {
        let mut reader = Mp4BoxReader::new(reader.compat());
        let _: Moov = reader.read_box().await?;
        let init_range = 0..reader.position();
        let sidx = reader.read_box::<Sidx>().await?;
        let sidx_end_offset = reader.position();

        Self::from_sidx(init_range, sidx, sidx_end_offset)
    }
}

fn parse_sidx_references(
    sidx: &Sidx,
    sidx_end_offset: u64,
    init_segment: Arc<InitSegment>,
) -> Result<Vec<Segment>, SegmentIndexError> {
    let mut start_ticks = sidx.earliest_presentation_time;
    let mut start_byte = sidx_end_offset
        .checked_add(sidx.first_offset)
        .ok_or_else(|| {
            SegmentIndexError::InvalidSidx("first offset overflows the byte range".into())
        })?;
    let mut segments = Vec::with_capacity(sidx.references.len());

    for (index, reference) in sidx.references.iter().enumerate() {
        let end_ticks = start_ticks
            .checked_add(u64::from(reference.subsegment_duration))
            .ok_or_else(|| {
                SegmentIndexError::InvalidSidx(format!("segment {index} time range overflows"))
            })?;
        let end_byte = start_byte
            .checked_add(u64::from(reference.reference_size))
            .ok_or_else(|| {
                SegmentIndexError::InvalidSidx(format!("segment {index} byte range overflows"))
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
