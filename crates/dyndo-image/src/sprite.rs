//! Sprite sheets cut from a video track: which frames one shows, where their bytes
//! are, and the two reads that fetch them.
//!
//! A thumbnail track is not a track dyndo stores: it is a grid of frames cut from a
//! video track at a fixed cadence, addressed by the presentation time its first
//! cell shows. A sprite's duration follows from its cadence as `cells * cadence`, so
//! every one covers the same span and a manifest can describe the whole track with
//! one repeated timeline entry.

use std::ops::Range;

use bytes::Bytes;
use dyndo_core::asset_descriptor::TrackKind;
use dyndo_core::segment::{Segment, SegmentOptions};
use dyndo_core::track::Track;
use opendal::Operator;

use crate::ThumbnailError;
use crate::avc_decoder::AvcDecoder;
use crate::image::Image;

/// The sample entry of the only codec a sprite can be cut from.
const AVC_SAMPLE_ENTRY: &str = "avc1";

/// Whether a sheet can be cut from a track of this codec, so a caller choosing
/// between renditions can skip the ones that would only be refused.
pub fn supports(codec: &str) -> bool {
    codec.starts_with(AVC_SAMPLE_ENTRY)
}

/// Each cell's segment, as a byte range relative to the start of the one range the
/// whole sheet is read from, and `None` for a cell holding no frame.
type Cells = Vec<Option<Range<u64>>>;

/// A sprite sheet to cut: how its thumbnails are laid out, and how far apart in the
/// presentation they are taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sprite {
    /// Thumbnails per row, and per column.
    ///
    /// A player reads this as the value of the DASH-IF `thumbnail_tile` essential
    /// property — `5` becomes `5x5` — and divides the sheet by it to place a cell.
    pub grid: u32,
    /// The width one thumbnail is scaled to; its height follows the source's aspect.
    pub cell_width: u32,
    /// Milliseconds between one thumbnail and the next.
    pub cadence: u32,
}

impl Sprite {
    /// The presentation one sheet covers, in milliseconds.
    pub fn duration(&self) -> u64 {
        u64::from(self.cadence) * u64::from(self.cells())
    }

    /// The pixel size of one sheet cut from a `width`×`height` video track, which is
    /// what a manifest advertises as the thumbnail representation's dimensions.
    pub fn size(&self, source: (u32, u32)) -> (u32, u32) {
        Image::size(self.grid, self.cell_width, source)
    }

    /// Cuts the sheet whose first cell shows `time`, in milliseconds from the start
    /// of the presentation.
    ///
    /// Two range reads fetch everything it needs: the track's initialization segment
    /// for the parameter sets a frame decodes against, and one contiguous range
    /// holding every segment the cells fall in.
    ///
    /// # Errors
    ///
    /// Returns a [`ThumbnailError`] when the track is not AVC video, when no sheet
    /// starts at `time`, or when a frame cannot be read, decoded, or encoded.
    pub async fn generate(
        &self,
        op: &Operator,
        track: &Track,
        options: &SegmentOptions,
        time: u64,
    ) -> Result<Bytes, ThumbnailError> {
        let TrackKind::Video(video) = track.kind() else {
            return Err(ThumbnailError::NotVideo(track.id()));
        };
        if !supports(track.codec()) {
            return Err(ThumbnailError::UnsupportedCodec(track.codec().to_string()));
        }
        let (range, cells) = self
            .window(track, time)
            .ok_or(ThumbnailError::NotFound(time))?;

        let initialization = track.read_initialization(op, options).await?;
        let media = track.read_range(op, options, range).await?;

        let sprite = *self;
        let source = (video.width, video.height);

        // Decoding a sheet's frames and encoding it is hundreds of milliseconds of
        // CPU, which on the caller's executor would stall every request sharing its
        // thread.
        tokio::task::spawn_blocking(move || {
            compose(&initialization, &media, &cells, sprite, source)
        })
        .await
        .expect("composing a sheet does not panic")
    }

    /// Thumbnails per sheet.
    fn cells(&self) -> u32 {
        self.grid * self.grid
    }

    /// The one contiguous byte range holding every segment the sheet's cells fall
    /// in, and each cell's segment relative to the start of it.
    ///
    /// Segments run in presentation order, so a sheet's cells are always covered by
    /// a single range. A cell is `None` once the presentation ends before the time it
    /// would show, which is how the trailing sheet of an asset comes out partly
    /// filled; two cells name the same segment when the cadence is shorter than one,
    /// since only the frame a segment opens on is decoded out of it.
    ///
    /// Returns `None` when the sheet is addressed at nothing: a cadence or a grid of
    /// zero asks for no thumbnails at all, `time` has to be one a sheet starts at,
    /// and the presentation has to reach it.
    fn window(&self, track: &Track, time: u64) -> Option<(Range<u64>, Cells)> {
        let duration = self.duration();
        if duration == 0 || !time.is_multiple_of(duration) {
            return None;
        }

        // Default options group nothing, so each of these is one stored fragment. A
        // sheet's cadence is its own: it must not shift with the segmentation a
        // request asks for delivery in.
        let segments = track.segments(&SegmentOptions::default());
        let timescale = track.timescale();
        let found: Cells = (0..u64::from(self.cells()))
            .map(|cell| segment_at(&segments, timescale, time + cell * u64::from(self.cadence)))
            .collect();
        let start = found.iter().flatten().map(|range| range.start).min()?;
        let end = found.iter().flatten().map(|range| range.end).max()?;

        Some((
            start..end,
            found
                .into_iter()
                .map(|segment| segment.map(|range| range.start - start..range.end - start))
                .collect(),
        ))
    }
}

/// Decodes one frame per cell and lays them out into a single image.
fn compose(
    initialization: &[u8],
    media: &[u8],
    cells: &[Option<Range<u64>>],
    sprite: Sprite,
    source: (u32, u32),
) -> Result<Bytes, ThumbnailError> {
    let mut decoder = AvcDecoder::new(initialization)?;
    let mut image = Image::new(sprite.grid, sprite.cell_width, source);

    for (index, cell) in cells.iter().enumerate() {
        // A cell the presentation never reaches stays black. DASH-IF expects a
        // trailing sheet to be partly filled, and a player placing a cell by time
        // never asks for one of them.
        let Some(range) = cell else { continue };
        let fragment = media.get(range.start as usize..range.end as usize).ok_or(
            ThumbnailError::Container("cell falls outside the range read"),
        )?;
        let index = u32::try_from(index).expect("a sheet holds no more cells than its grid");
        image.place(index, decoder.frame(fragment)?)?;
    }

    image.encode()
}

/// The byte range of the segment covering `at` milliseconds into the presentation,
/// or `None` when the track ends before it.
///
/// Segment presentation times are cumulative rather than stored, and run from the
/// track's earliest presentation time — which is what a manifest hands a player as
/// the thumbnail timeline's zero — so a time converts straight into an offset from
/// the first segment with no anchor to add back in.
fn segment_at(segments: &[Segment], timescale: u32, at: u64) -> Option<Range<u64>> {
    let raw = u128::from(at) * u128::from(timescale) / 1000;
    let mut elapsed = 0u128;

    for segment in segments {
        elapsed += u128::from(segment.raw_duration());
        if raw < elapsed {
            return Some(segment.byte_range());
        }
    }

    None
}

/// The AVC fixture declares 715 fragments of 1.92s at timescale 90000 — 1370.32s of
/// presentation, and with no grouping one segment each — so a 10s cadence puts a
/// thumbnail on every fifth segment and, across a 5×5 grid, a sheet on every 250s.
#[cfg(test)]
mod tests {
    use opendal::services::Memory;
    use relative_path::RelativePath;

    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");

    const SPRITE: Sprite = Sprite {
        grid: 5,
        cell_width: 320,
        cadence: 10_000,
    };

    #[test]
    fn duration_is_every_cell_at_the_cadence() {
        assert_eq!(SPRITE.duration(), 250_000);
    }

    #[test]
    fn duration_follows_the_grid_it_is_given() {
        assert_eq!(Sprite { grid: 4, ..SPRITE }.duration(), 160_000);
    }

    #[test]
    fn size_is_the_grid_of_cells() {
        assert_eq!(SPRITE.size((1920, 1080)), (1600, 900));
    }

    #[tokio::test]
    async fn window_rejects_a_cadence_asking_for_no_thumbnails() {
        let (_, track) = probe("video_avc_1080.mp4").await;

        assert!(
            Sprite {
                cadence: 0,
                ..SPRITE
            }
            .window(&track, 0)
            .is_none()
        );
    }

    #[tokio::test]
    async fn window_rejects_a_time_no_sheet_starts_at() {
        let (_, track) = probe("video_avc_1080.mp4").await;

        assert!(SPRITE.window(&track, SPRITE.cadence.into()).is_none());
    }

    #[tokio::test]
    async fn window_rejects_a_sheet_the_presentation_never_reaches() {
        let (_, track) = probe("video_avc_1080.mp4").await;

        assert!(SPRITE.window(&track, SPRITE.duration() * 6).is_none());
    }

    #[tokio::test]
    async fn window_fills_every_cell_of_a_sheet_inside_the_presentation() {
        let (_, track) = probe("video_avc_1080.mp4").await;

        let (_, cells) = SPRITE.window(&track, 0).unwrap();

        assert_eq!(
            (cells.len(), cells.iter().all(Option::is_some)),
            (SPRITE.cells() as usize, true)
        );
    }

    /// The fixture ends 120.32s into its sixth sheet, which is where a cadence stops
    /// finding frames to show.
    #[tokio::test]
    async fn window_leaves_cells_past_the_end_of_the_presentation_empty() {
        let (_, track) = probe("video_avc_1080.mp4").await;

        let (_, cells) = SPRITE.window(&track, SPRITE.duration() * 5).unwrap();

        assert_eq!(cells.iter().filter(|cell| cell.is_some()).count(), 13);
    }

    #[tokio::test]
    async fn window_reads_one_range_spanning_the_segments_its_cells_fall_in() {
        let (_, track) = probe("video_avc_1080.mp4").await;
        let segments = track.segments(&SegmentOptions::default());

        let (range, _) = SPRITE.window(&track, 0).unwrap();

        assert_eq!(
            range,
            segments[0].byte_range().start..segments[125].byte_range().end
        );
    }

    /// At a cadence of five segments, the cells step through the segment list five at
    /// a time — placed relative to the start of the one range that holds them.
    #[tokio::test]
    async fn window_places_each_cell_relative_to_that_range() {
        let (_, track) = probe("video_avc_1080.mp4").await;
        let segments = track.segments(&SegmentOptions::default());
        let start = segments[0].byte_range().start;

        let (_, cells) = SPRITE.window(&track, 0).unwrap();

        assert_eq!(
            (&cells[0], &cells[1]),
            (
                &Some(0..segments[0].byte_range().end - start),
                &Some(segments[5].byte_range().start - start..segments[5].byte_range().end - start)
            )
        );
    }

    /// Below a segment's duration the cadence asks for frames between one keyframe
    /// and the next, and only a keyframe is decoded, so consecutive cells land on the
    /// same segment.
    #[tokio::test]
    async fn window_repeats_a_segment_for_a_cadence_shorter_than_it() {
        let (_, track) = probe("video_avc_1080.mp4").await;

        let (_, cells) = Sprite {
            cadence: 1_000,
            ..SPRITE
        }
        .window(&track, 0)
        .unwrap();

        assert_eq!(cells[0], cells[1]);
    }

    /// The grid is the caller's to pick, and every derived quantity follows it.
    #[tokio::test]
    async fn window_of_a_smaller_grid_holds_fewer_cells() {
        let (_, track) = probe("video_avc_1080.mp4").await;
        let sprite = Sprite { grid: 3, ..SPRITE };

        let (_, cells) = sprite.window(&track, sprite.duration()).unwrap();

        assert_eq!((cells.len(), sprite.duration()), (9, 90_000));
    }

    #[test]
    fn compose_refuses_a_cell_pointing_outside_the_bytes_read() {
        let initialization = std::fs::read(format!("{FIXTURES}/video_avc_1080.mp4")).unwrap();

        let error =
            compose(&initialization, &[], &[Some(0..10)], SPRITE, (1920, 1080)).unwrap_err();

        assert!(matches!(error, ThumbnailError::Container(_)), "{error}");
    }

    async fn probe(name: &str) -> (Operator, Track) {
        let op = Operator::new(Memory::default()).unwrap();
        op.write(name, std::fs::read(format!("{FIXTURES}/{name}")).unwrap())
            .await
            .unwrap();
        let track = Track::probe(
            &op,
            RelativePath::new(name),
            None,
            &SegmentOptions::default(),
        )
        .await
        .unwrap();

        (op, track)
    }
}
