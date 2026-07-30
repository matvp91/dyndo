//! On-demand probing of a CMAF track's initialization and segment index.

use std::ops::Range;

use mp4_atom::Sidx;
use opendal::Operator;
use relative_path::RelativePath;

use super::box_reader;
use super::error::{Error, InvalidTrack};
use super::segment_index::{Segment, SegmentIndex, SegmentNotFound};

/// The initialization range and indexed media segments of a CMAF track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmafHeader {
    /// Byte range containing the track initialization data.
    pub initialization: Range<u64>,
    /// Units per second used by segment start times and durations.
    pub timescale: u32,
    /// Presentation time of the first segment.
    pub earliest_presentation_time: u64,
    /// Media segments in presentation order.
    pub segments: Vec<CmafSegment>,
}

/// A CMAF segment's timing and location in its source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmafSegment {
    /// The segment's position on the media timeline.
    pub timing: Segment,
    /// The segment's byte range in the source file.
    pub range: Range<u64>,
}

impl CmafHeader {
    /// Probe the CMAF header at `path` without reading media payloads.
    ///
    /// # Errors
    /// Returns an error when storage cannot be read, required MP4 boxes are
    /// invalid or absent, or the `sidx` uses hierarchical references.
    pub async fn read(op: &Operator, path: &RelativePath) -> Result<Self, Error> {
        let boxes = box_reader::scan(op, path).await?;
        let segments = segments_from_sidx(&boxes.sidx, boxes.sidx_end, path)?;

        Ok(Self {
            initialization: 0..boxes.moov_end,
            timescale: boxes.sidx.timescale,
            earliest_presentation_time: boxes.sidx.earliest_presentation_time,
            segments,
        })
    }

    /// Return the segment starting exactly at `start`.
    ///
    /// # Errors
    /// Returns [`SegmentNotFound`] when `start` is not an advertised segment
    /// boundary.
    pub fn segment_at(&self, start: u64) -> Result<&CmafSegment, SegmentNotFound> {
        self.segments
            .iter()
            .find(|segment| segment.timing.start == start)
            .ok_or(SegmentNotFound { start })
    }

    /// Return the format-independent timing index for this track.
    pub fn segment_index(&self) -> SegmentIndex {
        SegmentIndex {
            timescale: self.timescale,
            segments: self.segments.iter().map(|segment| segment.timing).collect(),
        }
    }
}

fn segments_from_sidx(
    sidx: &Sidx,
    sidx_end: u64,
    path: &RelativePath,
) -> Result<Vec<CmafSegment>, Error> {
    let mut offset = checked_add(sidx_end, sidx.first_offset, path)?;
    let mut start = sidx.earliest_presentation_time;
    let mut segments = Vec::with_capacity(sidx.references.len());

    for reference in &sidx.references {
        if reference.reference_type {
            return Err(invalid_track(path, InvalidTrack::HierarchicalSegmentIndex));
        }

        let size = u64::from(reference.reference_size);
        let duration = u64::from(reference.subsegment_duration);
        let end = checked_add(offset, size, path)?;

        segments.push(CmafSegment {
            timing: Segment { start, duration },
            range: offset..end,
        });

        offset = end;
        start = checked_add(start, duration, path)?;
    }

    Ok(segments)
}

fn checked_add(left: u64, right: u64, path: &RelativePath) -> Result<u64, Error> {
    left.checked_add(right)
        .ok_or_else(|| invalid_track(path, InvalidTrack::SegmentIndexOverflow))
}

fn invalid_track(path: &RelativePath, reason: InvalidTrack) -> Error {
    Error::InvalidTrack {
        path: path.to_owned(),
        reason,
    }
}
