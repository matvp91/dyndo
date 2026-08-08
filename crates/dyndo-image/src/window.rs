//! Which frames a sprite shows, and where their bytes are.
//!
//! A sprite's cells step on from the time asked for. Turning those times into bytes is
//! all this does: which fragment holds each cell's frame, and where that fragment sits
//! in the track.

use std::ops::Range;

use dyndo_core::segment::{self, SegmentOptions};
use dyndo_core::track::Track;

/// One cell's frame: the byte range of the fragment holding it, and the time it is
/// shown at in the track's timescale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cell {
    pub(crate) segment: Range<u64>,
    pub(crate) time: u64,
}

/// The cells a sprite holds, each naming the bytes it is cut from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Window {
    pub(crate) cells: Vec<Option<Cell>>,
}

impl Window {
    /// The window `cells` frames are cut from, the first shown at `time` and each one
    /// after it `step` later, both in milliseconds from the start of the presentation.
    ///
    /// A cell reads only the fragment its own frame is in, so what a sprite costs
    /// follows the cells it holds rather than the span they are spread over — a step
    /// twice as long moves the frames apart without reading any more of the track.
    ///
    /// A cell is `None` once the presentation ends before the time it would show, which
    /// is how the trailing sprite of an asset comes out partly filled; two cells name
    /// the same fragment when the step is shorter than one, and each still shows its
    /// own frame out of it.
    ///
    /// Returns `None` when the sprite is addressed at nothing: a step or a count of
    /// zero asks for no thumbnails at all, and the presentation has to reach `time`.
    pub(crate) fn new(track: &Track, cells: u32, step: u32, time: u64) -> Option<Self> {
        if cells == 0 || step == 0 {
            return None;
        }

        // Default options group nothing, so each of these is one stored fragment. A
        // sprite's step is its own: it must not shift with the segmentation a request
        // asks for delivery in.
        let segments = segment::segments(track, &SegmentOptions::default());
        let timescale = track.timescale();
        let anchor = track.earliest_presentation_time();
        let cells: Vec<Option<Cell>> = (0..u64::from(cells))
            .map(|cell| {
                let time =
                    anchor.saturating_add(raw_time(time + cell * u64::from(step), timescale));
                segments
                    .iter()
                    .find(|segment| segment.raw_range().contains(&time))
                    .map(|segment| Cell {
                        segment: segment.byte_range(),
                        time,
                    })
            })
            .collect();
        cells.iter().flatten().next()?;

        Some(Self { cells })
    }
}

/// A presentation time in milliseconds, counted in the track's own timescale.
///
/// Rounded down, so a cell shows the frame on screen at the time it asks for rather
/// than the one after it.
fn raw_time(at_ms: u64, timescale: u32) -> u64 {
    u64::try_from(u128::from(at_ms) * u128::from(timescale) / 1000).unwrap_or(u64::MAX)
}

/// The AVC fixture declares 715 fragments of 1.92s at timescale 90000 — 1370.32s of
/// presentation, and with no grouping one segment each — so a 10s step puts a
/// thumbnail on every fifth segment and, across 25 cells, a sprite on every 250s.
#[cfg(test)]
mod tests {
    use opendal::Operator;
    use opendal::services::Memory;
    use relative_path::RelativePath;

    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");
    const CELLS: u32 = 25;
    const STEP: u32 = 10_000;

    #[tokio::test]
    async fn new_rejects_a_step_or_a_count_asking_for_no_thumbnails() {
        let track = probe("video_avc_1080.mp4").await;

        assert!(
            Window::new(&track, CELLS, 0, 0).is_none() && Window::new(&track, 0, STEP, 0).is_none()
        );
    }

    #[tokio::test]
    async fn new_rejects_a_sprite_the_presentation_never_reaches() {
        let track = probe("video_avc_1080.mp4").await;

        assert!(Window::new(&track, CELLS, STEP, 1_400_000).is_none());
    }

    /// A sprite is cut at the time asked for rather than at one the tile happens to
    /// land on, and its cells step on from there: 10s at a 2s step across four cells
    /// shows 10s, 12s, 14s and 16s.
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
    async fn new_fills_every_cell_of_a_sprite_inside_the_presentation() {
        let track = probe("video_avc_1080.mp4").await;

        let window = Window::new(&track, CELLS, STEP, 0).unwrap();

        assert_eq!(
            (window.cells.len(), window.cells.iter().all(Option::is_some)),
            (CELLS as usize, true)
        );
    }

    /// The fixture ends 120.32s into its sixth sprite, which is where the step stops
    /// finding frames to show.
    #[tokio::test]
    async fn new_leaves_cells_past_the_end_of_the_presentation_empty() {
        let track = probe("video_avc_1080.mp4").await;

        let window = Window::new(&track, CELLS, STEP, 1_250_000).unwrap();

        assert_eq!(
            window.cells.iter().filter(|cell| cell.is_some()).count(),
            13
        );
    }

    /// At a step of five segments, the cells walk the segment list five at a time, each
    /// naming the fragment it is cut from and nothing between them.
    #[tokio::test]
    async fn new_names_the_fragment_each_cell_is_cut_from() {
        let track = probe("video_avc_1080.mp4").await;
        let segments = segment::segments(&track, &SegmentOptions::default());

        let window = Window::new(&track, CELLS, STEP, 0).unwrap();

        assert_eq!(
            window
                .cells
                .iter()
                .flatten()
                .map(|cell| cell.segment.clone())
                .take(2)
                .collect::<Vec<_>>(),
            vec![segments[0].byte_range(), segments[5].byte_range()]
        );
    }

    /// What a sprite reads follows the cells it holds, not the span they cover: a step
    /// twice as long moves the frames apart without reading any more of the track.
    #[tokio::test]
    async fn a_longer_step_reads_no_more_than_a_shorter_one() {
        let track = probe("video_avc_1080.mp4").await;
        let read = |window: Window| -> u64 {
            window
                .cells
                .iter()
                .flatten()
                .map(|cell| cell.segment.end - cell.segment.start)
                .sum()
        };

        let short = Window::new(&track, CELLS, STEP, 0).unwrap();
        let long = Window::new(&track, CELLS, STEP * 3, 0).unwrap();

        assert!(read(long) < read(short) * 2);
    }

    /// Below a segment's duration the step asks for frames between one keyframe and
    /// the next, so consecutive cells read the same segment — and are decoded to
    /// different times inside it.
    #[tokio::test]
    async fn new_repeats_a_segment_for_a_step_shorter_than_it() {
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
