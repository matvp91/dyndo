//! What a request is served: fragments grouped into segments.
//!
//! A segment is derived, never stored — the same track cut under different options
//! yields different segments, so nothing holds them and every answer here is a
//! function of a track plus the options asked for.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::asset_descriptor::TrackKind;
use crate::boundary_utils::BoundaryUtils;
use crate::fragment::Fragment;
use crate::track::Track;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentOptions {
    /// The shortest a served segment may be, in milliseconds; fragments are
    /// grouped until they reach it.
    #[serde(default, alias = "sml", alias = "segment_min_length")]
    pub min_length: u32,
    /// How long each segment of a packaged subtitle track is, in milliseconds.
    /// Unlike `min_length` this is exact, since dyndo fragments those tracks
    /// itself rather than grouping what a file already contains. Zero asks for no
    /// grid, leaving the asset's splice points as the only cuts.
    #[serde(default, alias = "stl", alias = "segment_text_length")]
    pub text_length: u32,
    /// Times a segment has to start at, in milliseconds.
    #[serde(default, alias = "sb", alias = "segment_boundaries")]
    pub boundaries: Vec<u32>,
}

/// One served segment: where its bytes are, and when it is shown.
///
/// A segment sits at a position on two axes, and holds an extent on both. Its
/// presentation time is cumulative rather than stored — the track's earliest
/// presentation time plus every duration before it — so it follows from the
/// grouping that produced the segment and is settled there, once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    byte_offset: u64,
    byte_size: u64,
    raw_start: u64,
    raw_duration: u64,
}

impl Segment {
    /// A resource cut when it is asked for rather than stored has no bytes to point
    /// at, and passes zero for both.
    pub fn new(byte_offset: u64, byte_size: u64, raw_start: u64, raw_duration: u64) -> Self {
        Self {
            byte_offset,
            byte_size,
            raw_start,
            raw_duration,
        }
    }

    pub fn byte_range(&self) -> Range<u64> {
        self.byte_offset..self.byte_offset + self.byte_size
    }

    pub fn byte_size(&self) -> u64 {
        self.byte_size
    }

    /// Returns the presentation time the segment covers, in the track's timescale
    /// units. It begins at the track's earliest presentation time plus every duration
    /// before it, so consecutive segments meet without a gap.
    pub fn raw_range(&self) -> Range<u64> {
        self.raw_start..self.raw_start + self.raw_duration
    }

    /// Returns the segment's duration in the track's timescale units.
    pub fn raw_duration(&self) -> u64 {
        self.raw_duration
    }
}

/// Returns the segments `track` is served as under `options`.
pub fn segments(track: &Track, options: &SegmentOptions) -> Vec<Segment> {
    group(
        track.fragments(),
        track.timescale(),
        track.earliest_presentation_time(),
        options,
    )
}

/// Returns the segments that fall within `span_ms`, timed in `timescale` units.
///
/// Empty when there is nothing there — the segments ran out before the span opened, or
/// two boundaries snapped to the same edge — so callers pairing spans with segments
/// always get one list per span.
///
/// The first segment begins at or after the span's start rather than on it: segments
/// snap to their own nearest edge, so a span opens before some tracks have anything to
/// give. That segment carries the time it begins at, which is what a manifest reads the
/// timeline against.
pub fn span(segments: &[Segment], timescale: u32, span_ms: &Range<u32>) -> Vec<Segment> {
    let mut edges = Vec::with_capacity(segments.len() + 1);
    edges.push(0u64);
    for segment in segments {
        edges.push(edges[edges.len() - 1] + segment.raw_duration());
    }

    let start = BoundaryUtils::snap_cut(&edges, timescale, span_ms.start);
    let end = BoundaryUtils::snap_cut(&edges, timescale, span_ms.end).max(start);

    segments[start..end].to_vec()
}

/// Returns the longest audio or video segment duration in milliseconds.
pub fn max_segment_duration(tracks: &[Track], options: &SegmentOptions) -> u32 {
    tracks
        .iter()
        .filter(|track| matches!(track.kind(), TrackKind::Video(_) | TrackKind::Audio(_)))
        .flat_map(|track| {
            let timescale = track.timescale();
            segments(track, options).into_iter().map(move |segment| {
                // Rounded up: this sizes a client's buffer, which has to cover the
                // whole of the longest segment rather than most of it.
                let duration =
                    (u128::from(segment.raw_duration()) * 1000).div_ceil(u128::from(timescale));
                u32::try_from(duration).unwrap_or(u32::MAX)
            })
        })
        .max()
        .unwrap_or(0)
}

/// Returns the highest average grouped-segment bitrate in bits per second.
pub fn max_bitrate(track: &Track, options: &SegmentOptions) -> u64 {
    segments(track, options)
        .iter()
        .map(|segment| {
            let bits = u128::from(segment.byte_size()) * 8;
            let scaled_bits = bits * u128::from(track.timescale());
            let bitrate = scaled_bits.div_ceil(u128::from(segment.raw_duration()));
            u64::try_from(bitrate).unwrap_or(u64::MAX)
        })
        .max()
        .unwrap_or(0)
}

/// Returns the average bitrate of all grouped segments in bits per second.
pub fn average_bitrate(track: &Track, options: &SegmentOptions) -> u64 {
    let (byte_size, raw_duration) = segments(track, options).iter().fold(
        (0_u128, 0_u128),
        |(byte_size, raw_duration), segment| {
            (
                byte_size + u128::from(segment.byte_size()),
                raw_duration + u128::from(segment.raw_duration()),
            )
        },
    );
    if raw_duration == 0 {
        return 0;
    }

    let bits = byte_size * 8;
    let scaled_bits = bits * u128::from(track.timescale());
    u64::try_from(scaled_bits.div_ceil(raw_duration)).unwrap_or(u64::MAX)
}

/// Groups `fragments` into segments, each timed from `raw_anchor` — the track's
/// earliest presentation time, which every segment time in a manifest counts from.
fn group(
    fragments: &[Fragment],
    timescale: u32,
    raw_anchor: u64,
    options: &SegmentOptions,
) -> Vec<Segment> {
    if options.min_length == 0 {
        let mut raw_start = raw_anchor;
        return fragments
            .iter()
            .map(|fragment| {
                let segment = Segment::new(
                    fragment.byte_offset,
                    fragment.byte_size,
                    raw_start,
                    u64::from(fragment.raw_duration),
                );
                raw_start += u64::from(fragment.raw_duration);

                segment
            })
            .collect();
    }

    let minimum = u128::from(options.min_length) * u128::from(timescale);
    let mut cumulative = Vec::with_capacity(fragments.len() + 1);
    cumulative.push(0u64);
    for fragment in fragments {
        cumulative.push(cumulative[cumulative.len() - 1] + u64::from(fragment.raw_duration()));
    }
    let cuts = BoundaryUtils::snap_cuts(&cumulative, timescale, &options.boundaries);

    let mut segments = Vec::new();
    let mut start = 0;
    let mut next_cut = 0;
    for end in 1..=fragments.len() {
        while next_cut < cuts.len() && cuts[next_cut] <= start {
            next_cut += 1;
        }
        let raw_duration = cumulative[end] - cumulative[start];
        let long_enough = u128::from(raw_duration) * 1000 >= minimum;
        let at_cut = next_cut < cuts.len() && cuts[next_cut] == end;
        if long_enough || at_cut || end == fragments.len() {
            let byte_offset = fragments[start].byte_offset;
            let byte_end = fragments[end - 1].byte_range().end;
            segments.push(Segment::new(
                byte_offset,
                byte_end - byte_offset,
                // `cumulative` is already the table of elapsed durations, so the
                // segment's own start is the entry its first fragment sits at.
                raw_anchor + cumulative[start],
                raw_duration,
            ));
            start = end;
        }
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_descriptor::{TextKind, VideoKind};

    #[test]
    fn default_options_group_nothing_and_cut_text_only_at_splice_points() {
        let options = SegmentOptions::default();

        assert_eq!((options.min_length, options.text_length), (0, 0));
    }

    fn options(min_length: u32, boundaries: &[u32]) -> SegmentOptions {
        SegmentOptions {
            min_length,
            boundaries: boundaries.to_vec(),
            ..SegmentOptions::default()
        }
    }

    fn fragments(raw_durations: &[u32]) -> Vec<Fragment> {
        let mut byte_offset = 100;
        raw_durations
            .iter()
            .map(|&raw_duration| {
                let fragment = Fragment::new(byte_offset, 10, raw_duration).unwrap();
                byte_offset += 10;
                fragment
            })
            .collect()
    }

    #[test]
    fn zero_minimum_maps_each_fragment_to_a_segment() {
        let fragments = fragments(&[1000, 1000]);
        let segments = group(&fragments, 1000, 0, &options(0, &[]));

        assert_eq!(
            segments,
            vec![
                Segment::new(100, 10, 0, 1000),
                Segment::new(110, 10, 1000, 1000),
            ]
        );
    }

    #[test]
    fn fragments_are_grouped_until_the_minimum() {
        let fragments = fragments(&[1920, 1920, 1920, 1920]);
        let segments = group(&fragments, 1000, 0, &options(3_000, &[]));

        assert_eq!(
            segments
                .iter()
                .map(Segment::raw_duration)
                .collect::<Vec<_>>(),
            vec![3840, 3840]
        );
    }

    #[test]
    fn segment_closes_at_a_requested_boundary() {
        let fragments = fragments(&[1920, 1920, 120, 1800, 1920]);
        let segments = group(&fragments, 1000, 0, &options(3_000, &[3_960]));

        assert_eq!(
            segments
                .iter()
                .map(Segment::raw_duration)
                .collect::<Vec<_>>(),
            vec![3840, 120, 3720]
        );
    }

    #[test]
    fn empty_fragments_produce_no_segments() {
        assert!(group(&[], 1000, 0, &options(3_000, &[])).is_empty());
    }

    #[test]
    fn final_short_segment_is_preserved() {
        let fragments = fragments(&[2000, 2000, 500]);
        let segments = group(&fragments, 1000, 0, &options(3_000, &[]));

        assert_eq!(
            segments
                .iter()
                .map(Segment::raw_duration)
                .collect::<Vec<_>>(),
            vec![4000, 500]
        );
    }

    #[test]
    fn boundary_on_fragment_edge_closes_the_segment_at_that_edge() {
        let fragments = fragments(&[1000, 1000, 1000]);
        let segments = group(&fragments, 1000, 0, &options(5_000, &[2_000]));

        assert_eq!(
            segments
                .iter()
                .map(Segment::raw_duration)
                .collect::<Vec<_>>(),
            vec![2000, 1000]
        );
    }

    #[test]
    fn boundary_inside_a_splice_fragment_cuts_where_the_splice_does() {
        // A spliced track carries the tail of the outgoing part as a short
        // fragment, and the boundary its siblings splice at can land inside that
        // tail rather than on either edge of it.
        let fragments = fragments(&[92_160, 8_192, 83_968, 92_160]);
        let segments = group(&fragments, 48_000, 0, &options(3_000, &[2_000]));

        assert_eq!(
            segments
                .iter()
                .map(Segment::raw_duration)
                .collect::<Vec<_>>(),
            vec![100_352, 176_128]
        );
    }

    #[test]
    fn grouped_segment_spans_combined_byte_range() {
        let fragments = fragments(&[1000, 1000]);
        let segments = group(&fragments, 1000, 0, &options(2_000, &[]));

        assert_eq!(segments[0].byte_range(), 100..120);
    }

    /// A segment starts where its predecessors left off, so consecutive segments meet
    /// without a gap whatever their durations.
    #[test]
    fn a_segment_starts_where_the_previous_one_ended() {
        let fragments = fragments(&[1000, 1500, 1000]);

        let segments = group(&fragments, 1000, 0, &options(0, &[]));

        assert_eq!(
            segments.iter().map(Segment::raw_range).collect::<Vec<_>>(),
            vec![0..1000, 1000..2500, 2500..3500]
        );
    }

    /// Grouping several fragments into one segment times it from the first of them.
    #[test]
    fn a_grouped_segment_starts_at_its_first_fragment() {
        let fragments = fragments(&[1000, 1000, 1000, 1000]);

        let segments = group(&fragments, 1000, 0, &options(2_000, &[]));

        assert_eq!(
            segments.iter().map(Segment::raw_range).collect::<Vec<_>>(),
            vec![0..2000, 2000..4000]
        );
    }

    /// Segment times are what a manifest hands a player, and those are counted from
    /// the track's earliest presentation time rather than from zero.
    #[test]
    fn segment_times_are_counted_from_the_anchor() {
        let fragments = fragments(&[1000, 1000]);

        let ungrouped = group(&fragments, 1000, 9_000, &options(0, &[]));
        let grouped = group(&fragments, 1000, 9_000, &options(2_000, &[]));

        assert_eq!(
            (
                ungrouped.iter().map(Segment::raw_range).collect::<Vec<_>>(),
                grouped[0].raw_range()
            ),
            (vec![9_000..10_000, 10_000..11_000], 9_000..11_000)
        );
    }

    #[test]
    fn max_segment_duration_excludes_text_and_rounds_up() {
        let tracks = vec![
            Track::fake(video_kind(), 3, vec![Fragment::new(0, 10, 1).unwrap()]),
            track(text_kind(), 10_000),
        ];

        assert_eq!(
            max_segment_duration(&tracks, &SegmentOptions::default()),
            334
        );
    }

    #[test]
    fn max_bitrate_returns_highest_segment_rate() {
        let track = Track::fake(
            video_kind(),
            1_000,
            vec![
                Fragment::new(0, 1_000, 1_000).unwrap(),
                Fragment::new(1_000, 2_000, 1_000).unwrap(),
            ],
        );

        assert_eq!(max_bitrate(&track, &SegmentOptions::default()), 16_000);
    }

    #[test]
    fn average_bitrate_uses_all_segment_bytes_and_duration() {
        let track = Track::fake(
            video_kind(),
            1_000,
            vec![
                Fragment::new(0, 1_000, 1_000).unwrap(),
                Fragment::new(1_000, 2_000, 1_000).unwrap(),
            ],
        );

        assert_eq!(average_bitrate(&track, &SegmentOptions::default()), 12_000);
    }

    #[test]
    fn bitrates_are_zero_without_segments() {
        let track = Track::fake(video_kind(), 1_000, Vec::new());

        assert_eq!(
            (
                max_bitrate(&track, &SegmentOptions::default()),
                average_bitrate(&track, &SegmentOptions::default())
            ),
            (0, 0)
        );
    }

    /// A track of one fragment, so its single segment is the whole of it.
    fn track(kind: TrackKind, raw_duration: u32) -> Track {
        Track::fake(
            kind,
            1_000,
            vec![Fragment::new(0, 10, raw_duration).unwrap()],
        )
    }

    fn video_kind() -> TrackKind {
        TrackKind::Video(VideoKind {
            width: 1920,
            height: 1080,
            frame_rate: "25/1".to_string(),
        })
    }

    fn text_kind() -> TrackKind {
        TrackKind::Text(TextKind {
            language: "eng".parse().unwrap(),
            role: None,
        })
    }

    /// A track of `count` one-second segments, at a timescale where raw units
    /// are milliseconds so boundaries read directly as segment counts.
    fn span_track(count: u32) -> Track {
        let fragments = (0..count)
            .map(|index| Fragment::new(u64::from(index) * 10, 10, 1_000).unwrap())
            .collect();
        Track::fake(span_kind(), 1_000, fragments)
    }

    fn span_kind() -> TrackKind {
        TrackKind::Video(VideoKind {
            width: 256,
            height: 144,
            frame_rate: "25/1".to_string(),
        })
    }

    fn durations(track: &Track, options: &SegmentOptions, duration: u32) -> Vec<Vec<u64>> {
        BoundaryUtils::divide(&options.boundaries, duration)
            .iter()
            .map(|range| {
                span(&segments(track, options), track.timescale(), range)
                    .iter()
                    .map(Segment::raw_duration)
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_track_without_boundaries_gives_every_segment_to_one_group() {
        assert_eq!(
            durations(&span_track(4), &options(0, &[]), 4_000),
            vec![vec![1_000; 4]]
        );
    }

    #[test]
    fn a_boundary_splits_the_segments_in_two() {
        assert_eq!(
            durations(&span_track(4), &options(0, &[3_000]), 4_000),
            vec![vec![1_000, 1_000, 1_000], vec![1_000]]
        );
    }

    /// Each span's group opens on the segment the boundary cut at, which is the one
    /// carrying the time a manifest reads that group's timeline against.
    #[test]
    fn a_group_opens_on_the_segment_the_span_cut_at() {
        let track = span_track(4);
        let options = options(0, &[3_000]);

        assert_eq!(starts(&track, &options, 4_000), vec![0, 3_000]);
    }

    #[test]
    fn a_group_opens_on_a_segment_timed_from_the_earliest_presentation_time() {
        let track = span_track(4).fake_earliest_presentation_time(500);
        let options = options(0, &[3_000]);

        assert_eq!(starts(&track, &options, 4_000), vec![500, 3_500]);
    }

    /// The time each span's group opens at, taken from its first segment.
    fn starts(track: &Track, options: &SegmentOptions, duration: u32) -> Vec<u64> {
        BoundaryUtils::divide(&options.boundaries, duration)
            .iter()
            .filter_map(|range| {
                span(&segments(track, options), track.timescale(), range)
                    .first()
                    .map(|segment| segment.raw_range().start)
            })
            .collect()
    }

    #[test]
    fn a_boundary_inside_a_segment_splits_at_the_following_edge() {
        assert_eq!(
            durations(&span_track(3), &options(0, &[1_200]), 3_000),
            vec![vec![1_000, 1_000], vec![1_000]]
        );
    }

    /// The split follows the edges grouping produced, not the fragment edges
    /// underneath them, so a group never opens mid-segment.
    #[test]
    fn a_split_lands_on_a_grouped_segment_edge() {
        assert_eq!(
            durations(&span_track(4), &options(2_000, &[3_000]), 4_000),
            vec![vec![2_000, 1_000], vec![1_000]]
        );
    }

    /// Two boundaries inside the same segment want two spans but can only cut
    /// once, so the span between them is handed an empty group rather than the
    /// segments of its neighbour.
    #[test]
    fn boundaries_snapping_to_one_edge_leave_a_span_empty() {
        assert_eq!(
            durations(&span_track(4), &options(0, &[1_100, 1_200]), 4_000),
            vec![vec![1_000, 1_000], vec![], vec![1_000, 1_000]]
        );
    }

    #[test]
    fn a_track_ending_before_a_span_gives_it_nothing() {
        assert_eq!(
            durations(&span_track(2), &options(0, &[3_000]), 10_000),
            vec![vec![1_000, 1_000], vec![]]
        );
    }

    #[test]
    fn a_track_without_segments_gives_every_span_an_empty_group() {
        let track = Track::fake(span_kind(), 1_000, Vec::new());

        assert_eq!(
            durations(&track, &options(0, &[3_000]), 10_000),
            vec![Vec::<u64>::new(), Vec::new()]
        );
    }
}
