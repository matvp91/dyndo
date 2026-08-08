//! Boundary arithmetic: everything the times an asset asks to be cut at decide on
//! their own.
//!
//! Nothing here needs a track. A boundary divides the presentation into spans, and
//! lands on the nearest edge some run of durations offers — fragment edges when
//! fragments are grouped into segments, segment edges when those are split across
//! spans.
//!
//! Two clocks meet here, so both are named: `raw_*` counts a track's timescale units
//! and `*_ms` counts milliseconds. Anywhere else, a time is either already marked, a
//! [`Duration`](std::time::Duration), or an asset.json key documented where it is
//! declared.

use std::ops::Range;

pub struct BoundaryUtils;

impl BoundaryUtils {
    /// The spans `boundaries_ms` divide a presentation of `duration_ms` into.
    ///
    /// Boundaries outside the presentation are dropped rather than folded onto its
    /// edges, since a span of no length holds nothing. Without boundaries the whole
    /// presentation is one span.
    pub fn divide(boundaries_ms: &[u32], duration_ms: u32) -> Vec<Range<u32>> {
        let mut edges: Vec<u32> = boundaries_ms
            .iter()
            .copied()
            .filter(|&boundary| boundary > 0 && boundary < duration_ms)
            .collect();
        edges.sort_unstable();
        edges.dedup();

        let mut spans = Vec::with_capacity(edges.len() + 1);
        let mut start = 0;
        for edge in edges.into_iter().chain([duration_ms]) {
            spans.push(start..edge);
            start = edge;
        }

        spans
    }

    /// The edges `boundaries_ms` fall on, as indices into `cumulative_raw`.
    ///
    /// A boundary lands on the first edge at or after it rather than the nearest one, so a
    /// segment never opens on content from before the boundary. The nearest edge can be the
    /// one before, and a track spliced there carries the tail of the outgoing part as a
    /// short fragment the boundary falls inside — snapping back would open the new segment
    /// on that tail and leave the splice uncut.
    pub(crate) fn snap_cuts(
        cumulative_raw: &[u64],
        timescale: u32,
        boundaries_ms: &[u32],
    ) -> Vec<usize> {
        let mut cuts: Vec<usize> = boundaries_ms
            .iter()
            .map(|&boundary_ms| Self::snap_cut(cumulative_raw, timescale, boundary_ms))
            .collect();
        cuts.sort_unstable();
        cuts.dedup();
        cuts
    }

    /// The single edge `boundary_ms` falls on, as an index into `cumulative_raw`.
    ///
    /// The two are cross-multiplied rather than converted: a boundary that falls between
    /// two timescale units must not round onto an edge it sits before.
    pub(crate) fn snap_cut(cumulative_raw: &[u64], timescale: u32, boundary_ms: u32) -> usize {
        let target = u128::from(boundary_ms) * u128::from(timescale);
        let index = cumulative_raw
            .partition_point(|&raw_duration| u128::from(raw_duration) * 1000 < target);

        index.min(cumulative_raw.len() - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_boundary_inside_a_span_snaps_to_the_following_edge() {
        assert_eq!(
            BoundaryUtils::snap_cuts(&[0, 1000, 2000], 1000, &[1200]),
            vec![2]
        );
    }

    #[test]
    fn out_of_range_boundaries_snap_to_the_outer_edges() {
        assert_eq!(
            BoundaryUtils::snap_cuts(&[0, 1000, 2000], 1000, &[0, 9000]),
            vec![0, 2]
        );
    }

    #[test]
    fn unordered_duplicate_boundaries_produce_unique_sorted_cuts() {
        assert_eq!(
            BoundaryUtils::snap_cuts(&[0, 1000, 2000, 3000], 1000, &[2000, 1000, 1000]),
            vec![1, 2]
        );
    }

    #[test]
    fn a_presentation_without_boundaries_is_one_span() {
        assert_eq!(BoundaryUtils::divide(&[], 10_000), vec![0..10_000]);
    }

    #[test]
    fn a_boundary_opens_a_span_that_runs_to_the_next() {
        assert_eq!(
            BoundaryUtils::divide(&[3_000], 10_000),
            vec![0..3_000, 3_000..10_000]
        );
    }

    #[test]
    fn every_boundary_adds_a_span() {
        assert_eq!(
            BoundaryUtils::divide(&[3_000, 6_000], 10_000),
            vec![0..3_000, 3_000..6_000, 6_000..10_000]
        );
    }

    #[test]
    fn boundaries_outside_the_presentation_open_no_span() {
        assert_eq!(
            BoundaryUtils::divide(&[0, 10_000, 12_000], 10_000),
            vec![0..10_000]
        );
    }

    #[test]
    fn unordered_duplicate_boundaries_open_one_span_each() {
        assert_eq!(
            BoundaryUtils::divide(&[6_000, 3_000, 3_000], 10_000),
            vec![0..3_000, 3_000..6_000, 6_000..10_000]
        );
    }
}
