use std::collections::HashMap;

use super::{ResolvedCmafTrack, Segment};

impl ResolvedCmafTrack {
    /// Returns the dominant interval between independently decodable segments, in milliseconds.
    pub fn idr_cadence(&self) -> u64 {
        let ticks = self.unscaled_idr_cadence().unwrap_or(0);
        let timescale = u128::from(self.timescale());
        if timescale == 0 {
            return 0;
        }
        u64::try_from(u128::from(ticks).saturating_mul(1_000) / timescale).unwrap_or(u64::MAX)
    }

    /// Returns segments whose start times align with the dominant IDR cadence.
    ///
    /// Unlike [`Self::segments`], this excludes extra independently decodable segments created by
    /// splice points. For example, with a two-second cadence, `segments()` may contain segments
    /// starting at `0, 2, 4, 5, 6, 8` seconds because a splice introduced a boundary at 5 seconds.
    /// This iterator returns the segments starting at `0, 2, 4, 6, 8` seconds.
    pub(crate) fn cadence_aligned_segments(&self) -> impl Iterator<Item = &Segment> {
        let anchor = self.segments().first().map(Segment::unscaled_start_time);
        let cadence = self.unscaled_idr_cadence();

        self.segments().iter().filter(move |segment| {
            let (Some(anchor), Some(cadence)) = (anchor, cadence) else {
                return false;
            };
            let tolerance = cadence.div_ceil(100);
            let offset = segment.unscaled_start_time().saturating_sub(anchor);
            let remainder = offset % cadence;
            remainder <= tolerance || cadence - remainder <= tolerance
        })
    }

    fn unscaled_idr_cadence(&self) -> Option<u64> {
        let mut durations = HashMap::new();
        for segment in self.segments() {
            let duration = segment
                .unscaled_end_time()
                .saturating_sub(segment.unscaled_start_time());
            *durations.entry(duration).or_insert(0_usize) += 1;
        }
        durations
            .into_iter()
            .max_by_key(|&(duration, occurrences)| (occurrences, duration))
            .map(|(duration, _)| duration)
            .filter(|duration| *duration != 0)
    }
}
