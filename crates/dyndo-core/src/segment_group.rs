//! Divides a presentation at its boundaries, and a track's segments along with it.
//!
//! A boundary is where a segment has to start, so the spans between them are the
//! longest stretches nothing is asked to cut. Each track contributes one group per
//! span — grouping already ended a segment at every boundary, so the split falls
//! on a segment edge. What a span means is left to whoever asked for it; DASH
//! turns them into periods.

use std::ops::Range;

use crate::segment::{Segment, SegmentOptions, snap_cut};
use crate::track::Track;

/// One track's segments within a span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentGroup {
    segments: Vec<Segment>,
}

impl SegmentGroup {
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }
}

/// The spans `boundaries` divide a presentation of `duration` milliseconds into.
///
/// Boundaries outside the presentation are dropped rather than folded onto its
/// edges, since a span of no length holds nothing. Without boundaries the whole
/// presentation is one span.
pub fn spans(boundaries: &[u32], duration: u32) -> Vec<Range<u32>> {
    let mut edges: Vec<u32> = boundaries
        .iter()
        .copied()
        .filter(|&boundary| boundary > 0 && boundary < duration)
        .collect();
    edges.sort_unstable();
    edges.dedup();

    let mut spans = Vec::with_capacity(edges.len() + 1);
    let mut start = 0;
    for edge in edges.into_iter().chain([duration]) {
        spans.push(start..edge);
        start = edge;
    }

    spans
}

/// Returns the segments of `track` that fall within `span`.
///
/// The group is empty when the track has nothing there — it ended before the
/// span opened, or two boundaries snapped to the same segment edge — so callers
/// pairing spans with groups always get one for one.
///
/// Its first segment begins at or after the span's start rather than on it:
/// tracks snap to their own nearest segment edge, so a span opens before some of
/// them have anything to give. That segment carries the time it begins at, which
/// is what a manifest reads the group's timeline against.
pub fn group_segments(track: &Track, options: &SegmentOptions, span: &Range<u32>) -> SegmentGroup {
    let segments = track.segments(options);
    let mut edges = Vec::with_capacity(segments.len() + 1);
    edges.push(0u64);
    for segment in &segments {
        edges.push(edges[edges.len() - 1] + segment.raw_duration());
    }

    let start = snap_cut(&edges, track.timescale(), span.start);
    let end = snap_cut(&edges, track.timescale(), span.end).max(start);

    SegmentGroup {
        segments: segments[start..end].to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_descriptor::{TrackKind, VideoKind};
    use crate::track::{Fragment, Track};

    /// A track of `count` one-second segments, at a timescale where raw units
    /// are milliseconds so boundaries read directly as segment counts.
    fn track(count: u32) -> Track {
        let fragments = (0..count)
            .map(|index| Fragment::new(u64::from(index) * 10, 10, 1_000).unwrap())
            .collect();
        Track::fake(kind(), 1_000, fragments)
    }

    fn kind() -> TrackKind {
        TrackKind::Video(VideoKind {
            width: 256,
            height: 144,
            frame_rate: "25/1".to_string(),
        })
    }

    fn options(min_length: u32, boundaries: &[u32]) -> SegmentOptions {
        SegmentOptions {
            min_length,
            boundaries: boundaries.to_vec(),
            ..SegmentOptions::default()
        }
    }

    fn durations(track: &Track, options: &SegmentOptions, duration: u32) -> Vec<Vec<u64>> {
        spans(&options.boundaries, duration)
            .iter()
            .map(|span| {
                group_segments(track, options, span)
                    .segments()
                    .iter()
                    .map(Segment::raw_duration)
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_presentation_without_boundaries_is_one_span() {
        assert_eq!(spans(&[], 10_000), vec![0..10_000]);
    }

    #[test]
    fn a_boundary_opens_a_span_that_runs_to_the_next() {
        assert_eq!(spans(&[3_000], 10_000), vec![0..3_000, 3_000..10_000]);
    }

    #[test]
    fn every_boundary_adds_a_span() {
        assert_eq!(
            spans(&[3_000, 6_000], 10_000),
            vec![0..3_000, 3_000..6_000, 6_000..10_000]
        );
    }

    #[test]
    fn boundaries_outside_the_presentation_open_no_span() {
        assert_eq!(spans(&[0, 10_000, 12_000], 10_000), vec![0..10_000]);
    }

    #[test]
    fn unordered_duplicate_boundaries_open_one_span_each() {
        assert_eq!(
            spans(&[6_000, 3_000, 3_000], 10_000),
            vec![0..3_000, 3_000..6_000, 6_000..10_000]
        );
    }

    #[test]
    fn a_track_without_boundaries_gives_every_segment_to_one_group() {
        assert_eq!(
            durations(&track(4), &options(0, &[]), 4_000),
            vec![vec![1_000; 4]]
        );
    }

    #[test]
    fn a_boundary_splits_the_segments_in_two() {
        assert_eq!(
            durations(&track(4), &options(0, &[3_000]), 4_000),
            vec![vec![1_000, 1_000, 1_000], vec![1_000]]
        );
    }

    /// Each span's group opens on the segment the boundary cut at, which is the one
    /// carrying the time a manifest reads that group's timeline against.
    #[test]
    fn a_group_opens_on_the_segment_the_span_cut_at() {
        let track = track(4);
        let options = options(0, &[3_000]);

        assert_eq!(starts(&track, &options, 4_000), vec![0, 3_000]);
    }

    #[test]
    fn a_group_opens_on_a_segment_timed_from_the_earliest_presentation_time() {
        let track = track(4).fake_earliest_presentation_time(500);
        let options = options(0, &[3_000]);

        assert_eq!(starts(&track, &options, 4_000), vec![500, 3_500]);
    }

    /// The time each span's group opens at, taken from its first segment.
    fn starts(track: &Track, options: &SegmentOptions, duration: u32) -> Vec<u64> {
        spans(&options.boundaries, duration)
            .iter()
            .filter_map(|span| {
                group_segments(track, options, span)
                    .segments()
                    .first()
                    .map(|segment| segment.raw_time_range().start)
            })
            .collect()
    }

    #[test]
    fn a_boundary_inside_a_segment_splits_at_the_following_edge() {
        assert_eq!(
            durations(&track(3), &options(0, &[1_200]), 3_000),
            vec![vec![1_000, 1_000], vec![1_000]]
        );
    }

    /// The split follows the edges grouping produced, not the fragment edges
    /// underneath them, so a group never opens mid-segment.
    #[test]
    fn a_split_lands_on_a_grouped_segment_edge() {
        assert_eq!(
            durations(&track(4), &options(2_000, &[3_000]), 4_000),
            vec![vec![2_000, 1_000], vec![1_000]]
        );
    }

    /// Two boundaries inside the same segment want two spans but can only cut
    /// once, so the span between them is handed an empty group rather than the
    /// segments of its neighbour.
    #[test]
    fn boundaries_snapping_to_one_edge_leave_a_span_empty() {
        assert_eq!(
            durations(&track(4), &options(0, &[1_100, 1_200]), 4_000),
            vec![vec![1_000, 1_000], vec![], vec![1_000, 1_000]]
        );
    }

    #[test]
    fn a_track_ending_before_a_span_gives_it_nothing() {
        assert_eq!(
            durations(&track(2), &options(0, &[3_000]), 10_000),
            vec![vec![1_000, 1_000], vec![]]
        );
    }

    #[test]
    fn a_track_without_segments_gives_every_span_an_empty_group() {
        let track = Track::fake(kind(), 1_000, Vec::new());

        assert_eq!(
            durations(&track, &options(0, &[3_000]), 10_000),
            vec![Vec::<u64>::new(), Vec::new()]
        );
    }
}
