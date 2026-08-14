use std::ops::Range;

use futures_util::io::AsyncRead;
use mp4_atom::{Moov, Sidx};
use thiserror::Error;
use tokio_util::compat::FuturesAsyncReadCompatExt;

use crate::{mp4_box_reader::Mp4BoxReader, mp4_readable::Mp4Readable, segment::Segment};

pub struct SegmentIndex {
    init_range: Range<u64>,
    segments: Vec<Segment>,
}

#[derive(Debug, Error)]
pub enum SegmentIndexError {
    #[error("failed to read segment index")]
    ReadFailed,
    #[error("sidx timescale is zero")]
    ZeroTimescale,
    #[error("segment byte range overflows")]
    ByteRangeOverflow,
    #[error("segment time range overflows")]
    TimeOverflow,
}

impl SegmentIndex {
    pub fn init_range(&self) -> Range<u64> {
        self.init_range.clone()
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }
}

impl Mp4Readable for SegmentIndex {
    type Error = SegmentIndexError;

    async fn from_reader(reader: &mut (impl AsyncRead + Unpin)) -> Result<Self, Self::Error> {
        let mut reader = Mp4BoxReader::new(reader.compat());
        let _: Moov = reader
            .read_box()
            .await
            .map_err(|_| SegmentIndexError::ReadFailed)?;
        let init_range = 0..reader.position();
        let sidx = reader
            .read_box::<Sidx>()
            .await
            .map_err(|_| SegmentIndexError::ReadFailed)?;
        let sidx_end_offset = reader.position();

        Ok(Self {
            init_range,
            segments: parse_sidx_references(&sidx, sidx_end_offset)?,
        })
    }
}

fn parse_sidx_references(
    sidx: &Sidx,
    sidx_end_offset: u64,
) -> Result<Vec<Segment>, SegmentIndexError> {
    if sidx.timescale == 0 {
        return Err(SegmentIndexError::ZeroTimescale);
    }

    let mut unscaled_start_time = sidx.earliest_presentation_time;
    let mut start_byte = sidx_end_offset
        .checked_add(sidx.first_offset)
        .ok_or(SegmentIndexError::ByteRangeOverflow)?;
    let mut segments = Vec::with_capacity(sidx.references.len());

    for reference in &sidx.references {
        let unscaled_end_time = unscaled_start_time
            .checked_add(u64::from(reference.subsegment_duration))
            .ok_or(SegmentIndexError::TimeOverflow)?;
        let end_byte = start_byte
            .checked_add(u64::from(reference.reference_size))
            .ok_or(SegmentIndexError::ByteRangeOverflow)?;

        segments.push(Segment::new(
            unscaled_start_time,
            unscaled_end_time,
            sidx.timescale,
            start_byte..end_byte,
        ));

        unscaled_start_time = unscaled_end_time;
        start_byte = end_byte;
    }

    Ok(segments)
}
