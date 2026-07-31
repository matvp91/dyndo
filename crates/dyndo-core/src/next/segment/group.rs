use super::{Segment, SegmentIndex};
use crate::next::error::Error;

impl SegmentIndex {
    /// Combine consecutive segments until each reaches `minimum_length_ms`,
    /// without crossing an asset splice point.
    ///
    /// A source segment is never split. The final segment may be shorter when
    /// no source segments remain or a splice point forces a cut. Splice points
    /// are expressed in milliseconds from the start of the presentation and
    /// snap to the nearest source-segment boundary; ties snap earlier.
    ///
    /// # Errors
    /// Returns an error when the index has a zero timescale or grouped duration
    /// accumulation overflows.
    pub fn group(
        &self,
        minimum_length_ms: u64,
        boundaries_ms: &[u64],
    ) -> Result<SegmentIndex, Error> {
        if self.timescale == 0 {
            return Err(Error::ZeroSegmentTimescale);
        }
        if minimum_length_ms == 0 {
            return Ok(self.clone());
        }

        let minimum = u128::from(minimum_length_ms) * u128::from(self.timescale);
        let cumulative = cumulative_durations(&self.segments)?;
        let cuts = snap_boundaries(&cumulative, self.timescale, boundaries_ms);
        let mut grouped = Vec::new();
        let mut start = 0;
        let mut next_cut = 0;

        for end in 1..=self.segments.len() {
            while next_cut < cuts.len() && cuts[next_cut] <= start {
                next_cut += 1;
            }

            let duration = cumulative[end] - cumulative[start];
            let long_enough = u128::from(duration) * 1000 >= minimum;
            let at_cut = next_cut < cuts.len() && cuts[next_cut] == end;

            if long_enough || at_cut || end == self.segments.len() {
                grouped.push(Segment {
                    start: self.segments[start].start,
                    duration,
                });
                start = end;
            }
        }

        Ok(SegmentIndex {
            timescale: self.timescale,
            segments: grouped,
        })
    }
}

fn cumulative_durations(segments: &[Segment]) -> Result<Vec<u64>, Error> {
    let mut cumulative = Vec::with_capacity(segments.len() + 1);
    cumulative.push(0u64);

    for segment in segments {
        let duration = cumulative[cumulative.len() - 1]
            .checked_add(segment.duration)
            .ok_or(Error::SegmentDurationOverflow)?;
        cumulative.push(duration);
    }

    Ok(cumulative)
}

fn snap_boundaries(cumulative: &[u64], timescale: u32, boundaries_ms: &[u64]) -> Vec<usize> {
    let mut cuts: Vec<usize> = boundaries_ms
        .iter()
        .map(|&boundary_ms| {
            let target = u128::from(boundary_ms) * u128::from(timescale);
            let above = cumulative.partition_point(|&time| u128::from(time) * 1000 < target);

            if above == 0 {
                0
            } else if above == cumulative.len() {
                cumulative.len() - 1
            } else {
                let distance_below = target - u128::from(cumulative[above - 1]) * 1000;
                let distance_above = u128::from(cumulative[above]) * 1000 - target;
                if distance_below <= distance_above {
                    above - 1
                } else {
                    above
                }
            }
        })
        .collect();

    cuts.sort_unstable();
    cuts.dedup();
    cuts
}
