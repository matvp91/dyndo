//! Pure segment arithmetic: grouping raw CMAF fragments into served segments
//! and timescale-to-milliseconds conversion. No I/O; everything here operates
//! on [`Segment`] slices.

use crate::asset::Segment;

/// Convert a count of `timescale`-units to milliseconds, truncating toward zero.
pub(crate) fn units_to_ms(units: u64, timescale: u32) -> u64 {
    (units as u128 * 1000 / timescale as u128) as u64
}

/// Group a track's raw CMAF fragments into served segments. Splice points
/// (`segment_boundaries_ms`) partition the fragments — a served segment never
/// spans one — and within each partition whole fragments are accumulated
/// greedily until a group reaches `min_segment_length_ms`. The final group of
/// a partition may be shorter (short tail). A `min_segment_length_ms` of 0
/// disables grouping. Fragments are contiguous byte ranges, so a group is the
/// first fragment's offset with summed sizes and durations. All comparisons
/// are exact u128 integer math (`units * 1000` vs `ms * timescale`); summing
/// `duration_ms` keeps per-track totals drift-free because group boundaries
/// are a subset of the raw drift-free boundaries.
pub(crate) fn group_segments(
    raw: &[Segment],
    timescale: u32,
    segment_boundaries_ms: &[u64],
    min_segment_length_ms: u64,
) -> Vec<Segment> {
    if min_segment_length_ms == 0 {
        return raw.to_vec();
    }
    let min_target = min_segment_length_ms as u128 * timescale as u128;

    // cum[i] = presentation units before fragment i; cum[raw.len()] = total.
    let mut cum = Vec::with_capacity(raw.len() + 1);
    cum.push(0u64);
    for s in raw {
        cum.push(cum[cum.len() - 1] + s.duration);
    }
    let cuts = snap_cuts(&cum, timescale, segment_boundaries_ms);

    let mut out = Vec::new();
    let mut start = 0;
    let mut next_cut = 0;
    for end in 1..=raw.len() {
        while next_cut < cuts.len() && cuts[next_cut] <= start {
            next_cut += 1;
        }
        let group_units = cum[end] - cum[start];
        let long_enough = group_units as u128 * 1000 >= min_target;
        let at_cut = next_cut < cuts.len() && cuts[next_cut] == end;
        if long_enough || at_cut || end == raw.len() {
            out.push(Segment {
                offset: raw[start].offset,
                size: raw[start..end].iter().map(|s| s.size).sum(),
                duration: group_units,
                duration_ms: raw[start..end].iter().map(|s| s.duration_ms).sum(),
            });
            start = end;
        }
    }
    out
}

/// Snap each splice point to the nearest fragment boundary, returned as
/// ascending, deduplicated indices into the cumulative-units table `cum`
/// (index 0 = track start — a no-op cut, as is `cum.len() - 1`, the track
/// end). Exact integer comparison in u128; a tie snaps earlier. Splice points
/// are a set: order and duplicates in the descriptor don't matter, so the
/// result is sorted rather than the input rejected.
fn snap_cuts(cum: &[u64], timescale: u32, boundaries_ms: &[u64]) -> Vec<usize> {
    let mut cuts: Vec<usize> = boundaries_ms
        .iter()
        .map(|&splice_ms| {
            let target = splice_ms as u128 * timescale as u128;
            let i = cum.partition_point(|&c| (c as u128) * 1000 < target);
            if i == 0 {
                0
            } else if i == cum.len() {
                cum.len() - 1
            } else {
                let below = target - cum[i - 1] as u128 * 1000;
                let above = cum[i] as u128 * 1000 - target;
                if below <= above { i - 1 } else { i }
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

    /// A contiguous fragment run: `durations` in timescale units, each fragment
    /// 10 bytes, starting at byte 100 (mirroring data after an init segment).
    fn frags(timescale: u32, durations: &[u64]) -> Vec<Segment> {
        let mut offset = 100;
        let (mut acc_units, mut acc_ms) = (0u64, 0u64);
        durations
            .iter()
            .map(|&d| {
                acc_units += d;
                let boundary_ms = units_to_ms(acc_units, timescale);
                let s = Segment {
                    offset,
                    size: 10,
                    duration: d,
                    duration_ms: boundary_ms - acc_ms,
                };
                acc_ms = boundary_ms;
                offset += 10;
                s
            })
            .collect()
    }

    #[test]
    fn grouping_is_a_noop_without_a_minimum() {
        let raw = frags(90000, &[172800, 172800]);
        assert_eq!(group_segments(&raw, 90000, &[], 0), raw);
    }

    #[test]
    fn a_minimum_below_every_fragment_is_a_noop() {
        // 1.92s GOPs, min 0.5s: every fragment already satisfies the minimum.
        let raw = frags(90000, &[172800; 3]);
        assert_eq!(group_segments(&raw, 90000, &[], 500), raw);
    }

    #[test]
    fn grouping_pairs_fragments_to_reach_the_minimum() {
        // 1.92s GOPs, min 3s -> 3.84s pairs.
        let raw = frags(90000, &[172800; 4]);
        let out = group_segments(&raw, 90000, &[], 3000);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].duration, 345600);
        assert_eq!(out[0].offset, 100);
        assert_eq!(out[0].size, 20);
        assert_eq!(out[1].offset, 120);
    }

    #[test]
    fn a_group_closes_exactly_at_the_minimum() {
        // 1s fragments @ min 2s -> exact 2s groups, not 3s.
        let raw = frags(90000, &[90000; 4]);
        let out = group_segments(&raw, 90000, &[], 2000);
        assert_eq!(
            out.iter().map(|s| s.duration).collect::<Vec<_>>(),
            vec![180000, 180000]
        );
    }

    #[test]
    fn the_track_tail_may_be_shorter_than_the_minimum() {
        let raw = frags(90000, &[172800, 172800, 122400]);
        let out = group_segments(&raw, 90000, &[], 3000);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].duration, 122400); // 1.36s tail, alone
    }

    #[test]
    fn a_minimum_beyond_the_track_makes_one_segment() {
        let raw = frags(90000, &[172800; 3]);
        let out = group_segments(&raw, 90000, &[], 3_600_000);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].duration, 518400);
        assert_eq!(out[0].size, 30);
    }

    #[test]
    fn grouping_an_empty_track_yields_nothing() {
        assert!(group_segments(&[], 90000, &[], 3000).is_empty());
    }

    #[test]
    fn grouped_duration_ms_stays_drift_free() {
        // timescale 3, six 1-unit fragments (1/3s each), min 600ms -> groups of
        // 2 (2/3s = 666.67ms exact). Drift-free totals: 666+667+667 = 2000.
        let raw = frags(3, &[1, 1, 1, 1, 1, 1]);
        let out = group_segments(&raw, 3, &[], 600);
        let ms: Vec<u64> = out.iter().map(|s| s.duration_ms).collect();
        assert_eq!(ms.iter().sum::<u64>(), 2000);
        assert_eq!(ms, vec![666, 667, 667]);
    }

    #[test]
    fn a_group_never_crosses_a_splice_point() {
        // Four 1.92s GOPs, splice exactly at 3.84s: without the cut min 5s
        // would span it; with it, both partitions close at the splice.
        let raw = frags(90000, &[172800; 4]);
        let out = group_segments(&raw, 90000, &[3840], 5000);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].duration, 345600);
        assert_eq!(out[1].duration, 345600);
    }

    #[test]
    fn a_conditioning_fragment_before_a_splice_becomes_a_short_segment() {
        // The real-asset shape scaled down: 2x1.92s + 0.12s | splice | 1.8s + 1.92s.
        let raw = frags(90000, &[172800, 172800, 10800, 162000, 172800]);
        let out = group_segments(&raw, 90000, &[3960], 3000);
        let durs: Vec<u64> = out.iter().map(|s| s.duration).collect();
        assert_eq!(durs, vec![345600, 10800, 334800]); // 3.84s, 0.12s tail, 3.72s
    }

    #[test]
    fn a_splice_snaps_to_the_nearest_fragment_boundary() {
        // Boundaries at 0/1.92/3.84s; splice at 2.0s is nearer 1.92 than 3.84.
        let raw = frags(90000, &[172800, 172800]);
        let out = group_segments(&raw, 90000, &[2000], 4000);
        let durs: Vec<u64> = out.iter().map(|s| s.duration).collect();
        assert_eq!(durs, vec![172800, 172800]);
    }

    #[test]
    fn an_exact_tie_snaps_earlier() {
        // 2s fragments; splice at 3.0s ties between 2s and 4s -> snaps to 2s.
        let raw = frags(90000, &[180000, 180000, 180000]);
        let out = group_segments(&raw, 90000, &[3000], 10_000);
        let durs: Vec<u64> = out.iter().map(|s| s.duration).collect();
        assert_eq!(durs, vec![180000, 360000]);
    }

    #[test]
    fn a_splice_beyond_the_track_is_a_noop() {
        let raw = frags(90000, &[172800; 2]);
        let out = group_segments(&raw, 90000, &[999_000], 5000);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn two_splices_snapping_to_one_boundary_cut_once() {
        // Both 1.90s and 1.94s snap to the 1.92s boundary.
        let raw = frags(90000, &[172800; 4]);
        let out = group_segments(&raw, 90000, &[1900, 1940], 5000);
        let durs: Vec<u64> = out.iter().map(|s| s.duration).collect();
        assert_eq!(durs, vec![172800, 518400]);
    }

    #[test]
    fn boundary_order_does_not_matter() {
        // Splice points are a set; unsorted input cuts at the same points.
        let raw = frags(90000, &[172800; 6]);
        let sorted = group_segments(&raw, 90000, &[1920, 5760], 9000);
        let unsorted = group_segments(&raw, 90000, &[5760, 1920], 9000);
        assert_eq!(sorted, unsorted);
        assert_eq!(sorted.len(), 3);
    }

    #[test]
    fn boundaries_without_a_minimum_leave_fragments_as_is() {
        let raw = frags(90000, &[172800; 4]);
        let out = group_segments(&raw, 90000, &[3840], 0);
        assert_eq!(out, raw);
    }

    #[test]
    fn video_and_text_group_to_identical_boundary_times() {
        // Text durations are the whole-ms mirror of the video durations;
        // grouping decisions must match so DASH/HLS timelines stay aligned
        // across tracks.
        let video = frags(90000, &[172800, 172800, 10800, 162000, 172800]);
        let text = frags(1000, &[1920, 1920, 120, 1800, 1920]);
        let p = |ts_frags: &[Segment], ts: u32| -> Vec<u128> {
            let mut acc = 0u128;
            group_segments(ts_frags, ts, &[3960], 3000)
                .iter()
                .map(|s| {
                    acc += s.duration as u128 * 1000 / ts as u128;
                    acc
                })
                .collect()
        };
        assert_eq!(p(&video, 90000), p(&text, 1000));
    }
}
