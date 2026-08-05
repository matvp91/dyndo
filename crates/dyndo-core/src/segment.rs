use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::track::{Fragment, Track};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentOptions {
    /// The shortest a served segment may be; fragments are grouped until they
    /// reach it.
    #[serde(
        default,
        rename = "min_length",
        alias = "sml",
        alias = "segment_min_length"
    )]
    pub min_length_ms: u64,
    /// How long each segment of a packaged subtitle track is. Unlike
    /// `min_length_ms` this is exact, since dyndo fragments those tracks
    /// itself rather than grouping what a file already contains. Zero asks for no
    /// grid, leaving the asset's splice points as the only cuts.
    #[serde(
        default,
        rename = "text_length",
        alias = "stl",
        alias = "segment_text_length"
    )]
    pub text_length_ms: u64,
    /// Times a segment has to start at.
    #[serde(default, alias = "sb", alias = "segment_boundaries")]
    pub boundaries: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    byte_offset: u64,
    byte_size: u64,
    duration: u64,
}

impl Segment {
    pub fn byte_range(&self) -> Range<u64> {
        self.byte_offset..self.byte_offset + self.byte_size
    }

    pub fn byte_size(&self) -> u64 {
        self.byte_size
    }

    pub fn duration(&self) -> u64 {
        self.duration
    }
}

impl Track {
    /// Returns the segments produced from the track under `options`.
    pub fn segments(&self, options: &SegmentOptions) -> Vec<Segment> {
        group_fragments(self.fragments(), self.timescale(), options)
    }
}

fn group_fragments(
    fragments: &[Fragment],
    timescale: u32,
    options: &SegmentOptions,
) -> Vec<Segment> {
    if options.min_length_ms == 0 {
        return fragments
            .iter()
            .map(|fragment| Segment {
                byte_offset: fragment.byte_offset,
                byte_size: fragment.byte_size,
                duration: fragment.duration,
            })
            .collect();
    }

    let minimum = u128::from(options.min_length_ms) * u128::from(timescale);
    let mut cumulative = Vec::with_capacity(fragments.len() + 1);
    cumulative.push(0u64);
    for fragment in fragments {
        cumulative.push(cumulative[cumulative.len() - 1] + fragment.duration());
    }
    let cuts = snap_cuts(&cumulative, timescale, &options.boundaries);

    let mut segments = Vec::new();
    let mut start = 0;
    let mut next_cut = 0;
    for end in 1..=fragments.len() {
        while next_cut < cuts.len() && cuts[next_cut] <= start {
            next_cut += 1;
        }
        let duration = cumulative[end] - cumulative[start];
        let long_enough = u128::from(duration) * 1000 >= minimum;
        let at_cut = next_cut < cuts.len() && cuts[next_cut] == end;
        if long_enough || at_cut || end == fragments.len() {
            let byte_offset = fragments[start].byte_offset;
            let byte_end = fragments[end - 1].byte_range().end;
            segments.push(Segment {
                byte_offset,
                byte_size: byte_end - byte_offset,
                duration,
            });
            start = end;
        }
    }

    segments
}

fn snap_cuts(cumulative: &[u64], timescale: u32, boundaries_ms: &[u64]) -> Vec<usize> {
    let mut cuts: Vec<usize> = boundaries_ms
        .iter()
        .map(|&boundary_ms| {
            let target = u128::from(boundary_ms) * u128::from(timescale);
            let index =
                cumulative.partition_point(|&duration| u128::from(duration) * 1000 < target);
            if index == 0 {
                0
            } else if index == cumulative.len() {
                cumulative.len() - 1
            } else {
                let below = target - u128::from(cumulative[index - 1]) * 1000;
                let above = u128::from(cumulative[index]) * 1000 - target;
                if below <= above { index - 1 } else { index }
            }
        })
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    cuts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options_group_nothing_and_cut_text_only_at_splice_points() {
        let options = SegmentOptions::default();

        assert_eq!((options.min_length_ms, options.text_length_ms), (0, 0));
    }

    fn options(min_length_ms: u64, boundaries_ms: &[u64]) -> SegmentOptions {
        SegmentOptions {
            min_length_ms,
            boundaries: boundaries_ms.to_vec(),
            ..SegmentOptions::default()
        }
    }

    fn fragments(durations: &[u64]) -> Vec<Fragment> {
        let mut byte_offset = 100;
        durations
            .iter()
            .map(|&duration| {
                let fragment = Fragment::new(byte_offset, 10, duration).unwrap();
                byte_offset += 10;
                fragment
            })
            .collect()
    }

    #[test]
    fn zero_minimum_maps_each_fragment_to_a_segment() {
        let fragments = fragments(&[1000, 1000]);
        let segments = group_fragments(&fragments, 1000, &options(0, &[]));

        assert_eq!(
            segments,
            vec![
                Segment {
                    byte_offset: 100,
                    byte_size: 10,
                    duration: 1000,
                },
                Segment {
                    byte_offset: 110,
                    byte_size: 10,
                    duration: 1000,
                },
            ]
        );
    }

    #[test]
    fn fragments_are_grouped_until_the_minimum() {
        let fragments = fragments(&[1920, 1920, 1920, 1920]);
        let segments = group_fragments(&fragments, 1000, &options(3_000, &[]));

        assert_eq!(
            segments.iter().map(Segment::duration).collect::<Vec<_>>(),
            vec![3840, 3840]
        );
    }

    #[test]
    fn segment_closes_at_a_requested_boundary() {
        let fragments = fragments(&[1920, 1920, 120, 1800, 1920]);
        let segments = group_fragments(&fragments, 1000, &options(3_000, &[3_960]));

        assert_eq!(
            segments.iter().map(Segment::duration).collect::<Vec<_>>(),
            vec![3840, 120, 3720]
        );
    }

    #[test]
    fn empty_fragments_produce_no_segments() {
        assert!(group_fragments(&[], 1000, &options(3_000, &[])).is_empty());
    }

    #[test]
    fn final_short_segment_is_preserved() {
        let fragments = fragments(&[2000, 2000, 500]);
        let segments = group_fragments(&fragments, 1000, &options(3_000, &[]));

        assert_eq!(
            segments.iter().map(Segment::duration).collect::<Vec<_>>(),
            vec![4000, 500]
        );
    }

    #[test]
    fn boundary_on_fragment_edge_closes_the_segment_at_that_edge() {
        let fragments = fragments(&[1000, 1000, 1000]);
        let segments = group_fragments(&fragments, 1000, &options(5_000, &[2_000]));

        assert_eq!(
            segments.iter().map(Segment::duration).collect::<Vec<_>>(),
            vec![2000, 1000]
        );
    }

    #[test]
    fn equidistant_boundary_snaps_to_earlier_edge() {
        assert_eq!(snap_cuts(&[0, 1000, 2000], 1000, &[1500]), vec![1]);
    }

    #[test]
    fn out_of_range_boundaries_snap_to_track_edges() {
        assert_eq!(snap_cuts(&[0, 1000, 2000], 1000, &[0, 9000]), vec![0, 2]);
    }

    #[test]
    fn unordered_duplicate_boundaries_produce_unique_sorted_cuts() {
        assert_eq!(
            snap_cuts(&[0, 1000, 2000, 3000], 1000, &[2500, 1000, 1000]),
            vec![1, 2]
        );
    }

    #[test]
    fn grouped_segment_spans_combined_byte_range() {
        let fragments = fragments(&[1000, 1000]);
        let segments = group_fragments(&fragments, 1000, &options(2_000, &[]));

        assert_eq!(segments[0].byte_range(), 100..120);
    }
}
