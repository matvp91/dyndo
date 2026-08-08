use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::track::{Fragment, Track};

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
    pub fn byte_range(&self) -> Range<u64> {
        self.byte_offset..self.byte_offset + self.byte_size
    }

    pub fn byte_size(&self) -> u64 {
        self.byte_size
    }

    /// Returns the presentation time the segment covers, in the track's timescale
    /// units.
    pub fn raw_time_range(&self) -> Range<u64> {
        self.raw_start..self.raw_start + self.raw_duration
    }

    /// Returns the segment's duration in the track's timescale units.
    pub fn raw_duration(&self) -> u64 {
        self.raw_duration
    }
}

impl Track {
    /// Returns the segments produced from the track under `options`.
    pub fn segments(&self, options: &SegmentOptions) -> Vec<Segment> {
        group_fragments(
            self.fragments(),
            self.timescale(),
            self.earliest_presentation_time(),
            options,
        )
    }
}

/// Groups `fragments` into segments, each timed from `anchor` — the track's earliest
/// presentation time, which every segment time in a manifest is counted from.
fn group_fragments(
    fragments: &[Fragment],
    timescale: u32,
    anchor: u64,
    options: &SegmentOptions,
) -> Vec<Segment> {
    if options.min_length == 0 {
        let mut raw_start = anchor;
        return fragments
            .iter()
            .map(|fragment| {
                let segment = Segment {
                    byte_offset: fragment.byte_offset,
                    byte_size: fragment.byte_size,
                    raw_start,
                    raw_duration: u64::from(fragment.raw_duration),
                };
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
    let cuts = snap_cuts(&cumulative, timescale, &options.boundaries);

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
            segments.push(Segment {
                byte_offset,
                byte_size: byte_end - byte_offset,
                // `cumulative` is already the table of elapsed durations, so the
                // segment's own start is the entry its first fragment sits at.
                raw_start: anchor + cumulative[start],
                raw_duration,
            });
            start = end;
        }
    }

    segments
}

/// The edges the boundaries fall on, as indices into `cumulative`. Fragment
/// edges when grouping into segments, segment edges when grouping those further.
///
/// A boundary lands on the first edge at or after it rather than the nearest one,
/// so a segment never opens on content from before the boundary. The nearest edge
/// can be the one before, and a track spliced there carries the tail of the
/// outgoing part as a short fragment the boundary falls inside — snapping back
/// would open the new segment on that tail and leave the splice uncut.
fn snap_cuts(cumulative: &[u64], timescale: u32, boundaries: &[u32]) -> Vec<usize> {
    let mut cuts: Vec<usize> = boundaries
        .iter()
        .map(|&boundary| snap_cut(cumulative, timescale, boundary))
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    cuts
}

/// The single edge `boundary` falls on, as an index into `cumulative`.
pub(crate) fn snap_cut(cumulative: &[u64], timescale: u32, boundary: u32) -> usize {
    let target = u128::from(boundary) * u128::from(timescale);
    let index =
        cumulative.partition_point(|&raw_duration| u128::from(raw_duration) * 1000 < target);

    index.min(cumulative.len() - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let segments = group_fragments(&fragments, 1000, 0, &options(0, &[]));

        assert_eq!(
            segments,
            vec![
                Segment {
                    byte_offset: 100,
                    byte_size: 10,
                    raw_start: 0,
                    raw_duration: 1000,
                },
                Segment {
                    byte_offset: 110,
                    byte_size: 10,
                    raw_start: 1000,
                    raw_duration: 1000,
                },
            ]
        );
    }

    #[test]
    fn fragments_are_grouped_until_the_minimum() {
        let fragments = fragments(&[1920, 1920, 1920, 1920]);
        let segments = group_fragments(&fragments, 1000, 0, &options(3_000, &[]));

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
        let segments = group_fragments(&fragments, 1000, 0, &options(3_000, &[3_960]));

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
        assert!(group_fragments(&[], 1000, 0, &options(3_000, &[])).is_empty());
    }

    #[test]
    fn final_short_segment_is_preserved() {
        let fragments = fragments(&[2000, 2000, 500]);
        let segments = group_fragments(&fragments, 1000, 0, &options(3_000, &[]));

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
        let segments = group_fragments(&fragments, 1000, 0, &options(5_000, &[2_000]));

        assert_eq!(
            segments
                .iter()
                .map(Segment::raw_duration)
                .collect::<Vec<_>>(),
            vec![2000, 1000]
        );
    }

    #[test]
    fn boundary_inside_a_fragment_snaps_to_the_following_edge() {
        assert_eq!(snap_cuts(&[0, 1000, 2000], 1000, &[1200]), vec![2]);
    }

    #[test]
    fn boundary_inside_a_splice_fragment_cuts_where_the_splice_does() {
        // A spliced track carries the tail of the outgoing part as a short
        // fragment, and the boundary its siblings splice at can land inside that
        // tail rather than on either edge of it.
        let fragments = fragments(&[92_160, 8_192, 83_968, 92_160]);
        let segments = group_fragments(&fragments, 48_000, 0, &options(3_000, &[2_000]));

        assert_eq!(
            segments
                .iter()
                .map(Segment::raw_duration)
                .collect::<Vec<_>>(),
            vec![100_352, 176_128]
        );
    }

    #[test]
    fn out_of_range_boundaries_snap_to_track_edges() {
        assert_eq!(snap_cuts(&[0, 1000, 2000], 1000, &[0, 9000]), vec![0, 2]);
    }

    #[test]
    fn unordered_duplicate_boundaries_produce_unique_sorted_cuts() {
        assert_eq!(
            snap_cuts(&[0, 1000, 2000, 3000], 1000, &[2000, 1000, 1000]),
            vec![1, 2]
        );
    }

    #[test]
    fn grouped_segment_spans_combined_byte_range() {
        let fragments = fragments(&[1000, 1000]);
        let segments = group_fragments(&fragments, 1000, 0, &options(2_000, &[]));

        assert_eq!(segments[0].byte_range(), 100..120);
    }

    /// A segment's time range runs from where its predecessors left off to its own
    /// end, so consecutive segments meet without a gap.
    #[test]
    fn segments_run_from_where_the_previous_one_ended() {
        let fragments = fragments(&[1000, 1500, 1000]);

        let segments = group_fragments(&fragments, 1000, 0, &options(0, &[]));

        assert_eq!(
            segments
                .iter()
                .map(Segment::raw_time_range)
                .collect::<Vec<_>>(),
            vec![0..1000, 1000..2500, 2500..3500]
        );
    }

    /// Grouping several fragments into one segment times it from the first of them,
    /// not from the segment before it ending.
    #[test]
    fn a_grouped_segment_runs_from_its_first_fragment() {
        let fragments = fragments(&[1000, 1000, 1000, 1000]);

        let segments = group_fragments(&fragments, 1000, 0, &options(2_000, &[]));

        assert_eq!(
            segments
                .iter()
                .map(Segment::raw_time_range)
                .collect::<Vec<_>>(),
            vec![0..2000, 2000..4000]
        );
    }

    /// Segment times are what a manifest hands a player, and those are counted from
    /// the track's earliest presentation time rather than from zero.
    #[test]
    fn segment_times_are_counted_from_the_anchor() {
        let fragments = fragments(&[1000, 1000]);

        let grouped = group_fragments(&fragments, 1000, 9_000, &options(0, &[]));
        let combined = group_fragments(&fragments, 1000, 9_000, &options(2_000, &[]));

        assert_eq!(
            (
                grouped
                    .iter()
                    .map(Segment::raw_time_range)
                    .collect::<Vec<_>>(),
                combined[0].raw_time_range()
            ),
            (vec![9_000..10_000, 10_000..11_000], 9_000..11_000)
        );
    }
}
