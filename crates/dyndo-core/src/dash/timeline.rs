//! SegmentTimeline construction: run-length compaction of equal-duration segments.

use dash_mpd::S;

use crate::cmaf::Segment;

/// Compact `segments` into `<S t d r>` runs. Only the first run carries `t`
/// (= `first_t`, the earliest presentation time); the rest are contiguous.
pub(crate) fn build_timeline(segments: &[Segment], first_t: u64) -> Vec<S> {
    let mut out: Vec<S> = Vec::new();
    let mut first = true;
    for seg in segments {
        match out.last_mut() {
            Some(last) if last.d == seg.duration => {
                *last.r.get_or_insert(0) += 1;
            }
            _ => {
                out.push(S {
                    t: if first { Some(first_t) } else { None },
                    d: seg.duration,
                    r: None,
                    ..Default::default()
                });
                first = false;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(duration: u64) -> Segment {
        Segment { offset: 0, size: 0, duration }
    }

    #[test]
    fn equal_durations_collapse_to_one_s_with_r() {
        let segs = vec![seg(100), seg(100), seg(100)];
        let out = build_timeline(&segs, 0);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].t, Some(0));
        assert_eq!(out[0].d, 100);
        assert_eq!(out[0].r, Some(2));
    }

    #[test]
    fn single_segment_has_no_r() {
        let out = build_timeline(&[seg(90)], 5);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].t, Some(5));
        assert_eq!(out[0].d, 90);
        assert_eq!(out[0].r, None);
    }

    #[test]
    fn varying_durations_split_runs_and_only_first_carries_t() {
        let segs = vec![seg(100), seg(100), seg(80), seg(100)];
        let out = build_timeline(&segs, 0);
        assert_eq!(out.len(), 3);
        assert_eq!((out[0].t, out[0].d, out[0].r), (Some(0), 100, Some(1)));
        assert_eq!((out[1].t, out[1].d, out[1].r), (None, 80, None));
        assert_eq!((out[2].t, out[2].d, out[2].r), (None, 100, None));
    }
}
