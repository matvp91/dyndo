//! Which frames a sheet shows, and where their bytes are.
//!
//! A sheet's cells step from the time asked for by its cadence. Turning those times
//! into bytes is all this does: the one contiguous range that has to be read, and
//! where inside it each cell's frame is stored.

use std::ops::Range;

use dyndo_core::segment::{Segment, SegmentOptions};
use dyndo_core::track::Track;

/// One cell's frame: the segment holding it, as a byte range relative to the start of
/// the window's own range, and the time it is shown at in the track's timescale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cell {
    pub(crate) segment: Range<u64>,
    pub(crate) time: u64,
}

/// The one range a sheet is read from, and the cells it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Window {
    pub(crate) range: Range<u64>,
    pub(crate) cells: Vec<Option<Cell>>,
}

impl Window {
    /// The window `cells` frames are cut from, the first shown at `time` and each one
    /// after it `cadence` later, both in milliseconds from the start of the
    /// presentation.
    ///
    /// Segments run in presentation order, so a sheet's cells are always covered by a
    /// single range. A cell is `None` once the presentation ends before the time it
    /// would show, which is how the trailing sheet of an asset comes out partly
    /// filled; two cells name the same segment when the cadence is shorter than one,
    /// and each still shows its own frame out of it.
    ///
    /// Returns `None` when the sheet is addressed at nothing: a cadence or a count of
    /// zero asks for no thumbnails at all, and the presentation has to reach `time`.
    pub(crate) fn new(track: &Track, cells: u32, cadence: u32, time: u64) -> Option<Self> {
        if cells == 0 || cadence == 0 {
            return None;
        }

        // Default options group nothing, so each of these is one stored fragment. A
        // sheet's cadence is its own: it must not shift with the segmentation a
        // request asks for delivery in.
        let segments = track.segments(&SegmentOptions::default());
        let timescale = track.timescale();
        let anchor = track.earliest_presentation_time();
        let found: Vec<Option<Cell>> = (0..u64::from(cells))
            .map(|cell| {
                let offset = raw_time(time + cell * u64::from(cadence), timescale);
                segment_at(&segments, offset).map(|segment| Cell {
                    segment,
                    // Segment times are cumulative from the track's earliest
                    // presentation time, while the fragment stamps its samples on the
                    // media clock a decoder has to be asked in.
                    time: anchor.saturating_add(offset),
                })
            })
            .collect();
        let start = found
            .iter()
            .flatten()
            .map(|cell| cell.segment.start)
            .min()?;
        let end = found.iter().flatten().map(|cell| cell.segment.end).max()?;

        Some(Self {
            range: start..end,
            cells: found
                .into_iter()
                .map(|cell| {
                    cell.map(|cell| Cell {
                        segment: cell.segment.start - start..cell.segment.end - start,
                        time: cell.time,
                    })
                })
                .collect(),
        })
    }
}

/// A presentation time in milliseconds, counted in the track's own timescale.
fn raw_time(at: u64, timescale: u32) -> u64 {
    u64::try_from(u128::from(at) * u128::from(timescale) / 1000).unwrap_or(u64::MAX)
}

/// The byte range of the segment covering `offset` timescale units into the
/// presentation, or `None` when the track ends before it.
///
/// Segment presentation times are cumulative rather than stored, and run from the
/// track's earliest presentation time — which is what a manifest hands a player as the
/// thumbnail timeline's zero — so an offset needs no anchor added back in to find the
/// segment holding it.
fn segment_at(segments: &[Segment], offset: u64) -> Option<Range<u64>> {
    let mut elapsed = 0u128;

    for segment in segments {
        elapsed += u128::from(segment.raw_duration());
        if u128::from(offset) < elapsed {
            return Some(segment.byte_range());
        }
    }

    None
}

/// The AVC fixture declares 715 fragments of 1.92s at timescale 90000 — 1370.32s of
/// presentation, and with no grouping one segment each — so a 10s cadence puts a
/// thumbnail on every fifth segment and, across 25 cells, a sheet on every 250s.
#[cfg(test)]
mod tests {
    use opendal::Operator;
    use opendal::services::Memory;
    use relative_path::RelativePath;

    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");
    const CELLS: u32 = 25;
    const CADENCE: u32 = 10_000;

    #[tokio::test]
    async fn new_rejects_a_cadence_or_a_count_asking_for_no_thumbnails() {
        let track = probe("video_avc_1080.mp4").await;

        assert!(
            Window::new(&track, CELLS, 0, 0).is_none()
                && Window::new(&track, 0, CADENCE, 0).is_none()
        );
    }

    #[tokio::test]
    async fn new_rejects_a_sheet_the_presentation_never_reaches() {
        let track = probe("video_avc_1080.mp4").await;

        assert!(Window::new(&track, CELLS, CADENCE, 1_400_000).is_none());
    }

    /// A sheet is cut at the time asked for rather than at one the grid happens to
    /// land on, and its cells step from there by the cadence: 10s at a 2s cadence
    /// across four cells shows 10s, 12s, 14s and 16s.
    #[tokio::test]
    async fn new_shows_the_times_asked_for() {
        let track = probe("video_avc_1080.mp4").await;

        let window = Window::new(&track, 4, 2_000, 10_000).unwrap();

        assert_eq!(
            window
                .cells
                .iter()
                .flatten()
                .map(|cell| cell.time / 90)
                .collect::<Vec<_>>(),
            vec![10_000, 12_000, 14_000, 16_000]
        );
    }

    #[tokio::test]
    async fn new_fills_every_cell_of_a_sheet_inside_the_presentation() {
        let track = probe("video_avc_1080.mp4").await;

        let window = Window::new(&track, CELLS, CADENCE, 0).unwrap();

        assert_eq!(
            (window.cells.len(), window.cells.iter().all(Option::is_some)),
            (CELLS as usize, true)
        );
    }

    /// The fixture ends 120.32s into its sixth sheet, which is where a cadence stops
    /// finding frames to show.
    #[tokio::test]
    async fn new_leaves_cells_past_the_end_of_the_presentation_empty() {
        let track = probe("video_avc_1080.mp4").await;

        let window = Window::new(&track, CELLS, CADENCE, 1_250_000).unwrap();

        assert_eq!(
            window.cells.iter().filter(|cell| cell.is_some()).count(),
            13
        );
    }

    #[tokio::test]
    async fn new_reads_one_range_spanning_the_segments_its_cells_fall_in() {
        let track = probe("video_avc_1080.mp4").await;
        let segments = track.segments(&SegmentOptions::default());

        let window = Window::new(&track, CELLS, CADENCE, 0).unwrap();

        assert_eq!(
            window.range,
            segments[0].byte_range().start..segments[125].byte_range().end
        );
    }

    /// At a cadence of five segments, the cells step through the segment list five at a
    /// time — placed relative to the start of the one range that holds them.
    #[tokio::test]
    async fn new_places_each_cell_relative_to_that_range() {
        let track = probe("video_avc_1080.mp4").await;
        let segments = track.segments(&SegmentOptions::default());
        let start = segments[0].byte_range().start;

        let window = Window::new(&track, CELLS, CADENCE, 0).unwrap();

        assert_eq!(
            window
                .cells
                .iter()
                .flatten()
                .map(|cell| cell.segment.clone())
                .take(2)
                .collect::<Vec<_>>(),
            vec![
                0..segments[0].byte_range().end - start,
                segments[5].byte_range().start - start..segments[5].byte_range().end - start
            ]
        );
    }

    /// Below a segment's duration the cadence asks for frames between one keyframe and
    /// the next, so consecutive cells read the same segment — and are decoded to
    /// different times inside it.
    #[tokio::test]
    async fn new_repeats_a_segment_for_a_cadence_shorter_than_it() {
        let track = probe("video_avc_1080.mp4").await;

        let window = Window::new(&track, CELLS, 1_000, 0).unwrap();

        let (first, second) = (
            window.cells[0].as_ref().unwrap(),
            window.cells[1].as_ref().unwrap(),
        );
        assert_eq!(
            (first.segment == second.segment, second.time - first.time),
            (true, 90_000)
        );
    }

    async fn probe(name: &str) -> Track {
        let op = Operator::new(Memory::default()).unwrap();
        op.write(name, std::fs::read(format!("{FIXTURES}/{name}")).unwrap())
            .await
            .unwrap();

        Track::probe(
            &op,
            RelativePath::new(name),
            None,
            &SegmentOptions::default(),
        )
        .await
        .unwrap()
    }
}
