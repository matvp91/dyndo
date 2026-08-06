//! Dividing a [`Subtitle`] into the fragments a CMAF text track is built from,
//! and joining those fragments back into one.
//!
//! [`fragment`] and [`merge`] are inverses: dividing a subtitle and merging the
//! result returns the cues it was authored with. Only the timeline is reasoned
//! about here — a container decides what to make of the fragments.
//!
//! Every time here is a millisecond, as on [`Cue`].

use std::iter::successors;

use crate::subtitle::{Cue, Subtitle};

/// The cues on screen over `[start, end)`, in presentation order, or none where
/// nothing is.
///
/// No cue edge falls inside a sample, so each cue here is on screen for all of it
/// and a cue's own span is the run of consecutive samples it appears in — which is
/// how [`merge`] recovers it.
///
/// A cue carries the span it was authored with when [`fragment`] divided a subtitle.
/// One read back out of a container carries the sample's span instead, since nothing
/// there records the original. Only [`Cue::same_content`] decides whether a cue
/// continues into the next sample, so neither producer has to agree with the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    pub start: u32,
    pub end: u32,
    pub cues: Vec<Cue>,
}

impl Sample {
    pub fn duration(&self) -> u32 {
        self.end - self.start
    }
}

/// Consecutive samples covering `[start, end)` without holes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    pub start: u32,
    pub end: u32,
    pub samples: Vec<Sample>,
}

impl Fragment {
    pub fn duration(&self) -> u32 {
        self.end - self.start
    }
}

/// Divides `subtitle` into fragments ending at each splice point in `boundaries`
/// and at every multiple of `length`, tiling each into samples. A `length` of 0
/// asks for no grid, leaving the splice points as the only divisions.
///
/// The division times come from the asset's clock rather than from where the cues
/// happen to fall, so every text track of an asset divides identically and stays
/// segment-aligned with its siblings. A cue spanning a division is on screen in
/// the fragments on either side of it.
///
/// The fragments run back to back from 0 to the end of the last cue, so a
/// subtitle with no cues has nothing to divide and yields none at all.
pub fn fragment(subtitle: &Subtitle, boundaries: &[u32], length: u32) -> Vec<Fragment> {
    let end = subtitle.cues.iter().map(|cue| cue.end).max().unwrap_or(0);
    let clock = boundaries.iter().copied().chain(grid(length, end));

    intervals(0, end, clock)
        .into_iter()
        .map(|(start, end)| Fragment {
            start,
            end,
            samples: samples(subtitle, start, end),
        })
        .collect()
}

/// The samples tiling `[start, end)`: the cues reaching into it, cut wherever one
/// comes or goes.
fn samples(subtitle: &Subtitle, start: u32, end: u32) -> Vec<Sample> {
    let cues: Vec<&Cue> = subtitle
        .cues
        .iter()
        .filter(|cue| cue.start < end && cue.end > start)
        .collect();
    let edges = cues.iter().flat_map(|cue| [cue.start, cue.end]);

    intervals(start, end, edges)
        .into_iter()
        .map(|(start, end)| Sample {
            start,
            end,
            cues: cues
                .iter()
                .filter(|cue| cue.start <= start && cue.end > start)
                .map(|&cue| cue.clone())
                .collect(),
        })
        .collect()
}

/// The cues `fragments` carry, in presentation order: the inverse of [`fragment`].
///
/// A cue outlasting a sample appears in every one it covers, so each is followed
/// across consecutive samples and the run it forms becomes one cue spanning them
/// all. Runs are followed across fragment edges too, since a segment groups several
/// fragments and a cue may span one.
///
/// The cue spans the samples rather than whatever span it arrived with, because a
/// cue read out of a container has only the sample's. Continuation is decided by
/// [`Cue::same_content`] for the same reason.
///
/// Two cues carrying the same content back to back merge into one spanning both.
/// They were authored apart, but once the timeline is cut into samples nothing
/// between them says so, and they render alike either way.
pub fn merge(fragments: &[Fragment]) -> Subtitle {
    let mut cues: Vec<Cue> = Vec::new();
    // The cues the sample just read left on screen, as indices into `cues`.
    let mut open: Vec<usize> = Vec::new();

    for sample in fragments.iter().flat_map(|fragment| &fragment.samples) {
        let mut still_open = Vec::with_capacity(sample.cues.len());
        for cue in &sample.cues {
            let carried_over = open
                .iter()
                .copied()
                .find(|&open| cues[open].end == sample.start && cues[open].same_content(cue));

            match carried_over {
                Some(open) => {
                    cues[open].end = sample.end;
                    still_open.push(open);
                }
                None => {
                    cues.push(Cue {
                        start: sample.start,
                        end: sample.end,
                        ..cue.clone()
                    });
                    still_open.push(cues.len() - 1);
                }
            }
        }
        open = still_open;
    }

    cues.sort_by_key(|cue| (cue.start, cue.end));

    Subtitle { cues }
}

/// Every multiple of `length` below `end`. A `length` of 0 asks for no grid.
fn grid(length: u32, end: u32) -> impl Iterator<Item = u32> {
    successors((length > 0).then_some(length), move |time| {
        time.checked_add(length)
    })
    .take_while(move |&time| time < end)
}

/// The consecutive intervals `[start, end)` falls into when divided at each time
/// in `at`. A time outside it names no interval, and a repeated one divides once,
/// so both are ignored.
fn intervals(start: u32, end: u32, at: impl IntoIterator<Item = u32>) -> Vec<(u32, u32)> {
    let mut times: Vec<u32> = at
        .into_iter()
        .filter(|&time| time > start && time < end)
        .chain([start, end])
        .collect();
    times.sort_unstable();
    times.dedup();

    times.windows(2).map(|times| (times[0], times[1])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subtitle_without_divisions_is_one_fragment() {
        let subtitle = subtitle(&[(0, 1_000, "A"), (2_000, 3_000, "B")]);

        assert_eq!(spans(&fragment(&subtitle, &[], 0)), [(0, 3_000)]);
    }

    #[test]
    fn a_boundary_ends_a_fragment() {
        let subtitle = subtitle(&[(0, 6_000, "A")]);

        assert_eq!(
            spans(&fragment(&subtitle, &[2_000], 0)),
            [(0, 2_000), (2_000, 6_000)]
        );
    }

    #[test]
    fn a_length_ends_fragments_on_a_grid() {
        let subtitle = subtitle(&[(0, 1_000, "A"), (2_000, 3_000, "B"), (4_000, 5_000, "C")]);

        assert_eq!(
            spans(&fragment(&subtitle, &[], 2_000)),
            [(0, 2_000), (2_000, 4_000), (4_000, 5_000)]
        );
    }

    #[test]
    fn boundaries_and_the_grid_combine() {
        let subtitle = subtitle(&[(0, 10_000, "A")]);

        assert_eq!(
            spans(&fragment(&subtitle, &[7_400], 4_000)),
            [(0, 4_000), (4_000, 7_400), (7_400, 8_000), (8_000, 10_000)]
        );
    }

    #[test]
    fn divisions_outside_the_track_are_ignored() {
        let subtitle = subtitle(&[(0, 2_000, "A")]);

        assert_eq!(
            spans(&fragment(&subtitle, &[0, 2_000, 9_000], 2_000)),
            [(0, 2_000)]
        );
    }

    #[test]
    fn a_repeated_division_ends_one_fragment() {
        let subtitle = subtitle(&[(0, 4_000, "A")]);

        assert_eq!(
            spans(&fragment(&subtitle, &[2_000, 2_000], 2_000)),
            [(0, 2_000), (2_000, 4_000)]
        );
    }

    #[test]
    fn text_tracks_sharing_a_clock_divide_alike() {
        let one = subtitle(&[(0, 1_000, "a"), (5_000, 9_000, "b")]);
        let other = subtitle(&[(0, 4_000, "x"), (4_500, 9_000, "y"), (8_000, 9_000, "z")]);

        assert_eq!(
            spans(&fragment(&one, &[7_400], 3_000)),
            spans(&fragment(&other, &[7_400], 3_000))
        );
    }

    #[test]
    fn a_subtitle_without_cues_yields_no_fragments() {
        let subtitle = Subtitle::default();

        assert!(fragment(&subtitle, &[1_000], 1_000).is_empty());
    }

    #[test]
    fn samples_tile_a_fragment_without_holes() {
        let subtitle = subtitle(&[(0, 1_000, "A"), (2_000, 3_000, "B")]);

        assert_eq!(
            samples_of(&fragment(&subtitle, &[], 0)[0]),
            [
                (0, 1_000, vec!["A"]),
                (1_000, 2_000, Vec::new()),
                (2_000, 3_000, vec!["B"]),
            ]
        );
    }

    #[test]
    fn a_track_starting_after_time_zero_opens_with_an_empty_sample() {
        let subtitle = subtitle(&[(2_000, 3_000, "A")]);

        assert_eq!(
            samples_of(&fragment(&subtitle, &[], 0)[0])[0],
            (0, 2_000, Vec::new())
        );
    }

    #[test]
    fn cues_on_screen_together_share_a_sample() {
        let subtitle = subtitle(&[(0, 2_000, "A"), (1_000, 3_000, "B")]);

        assert_eq!(
            samples_of(&fragment(&subtitle, &[], 0)[0]),
            [
                (0, 1_000, vec!["A"]),
                (1_000, 2_000, vec!["A", "B"]),
                (2_000, 3_000, vec!["B"]),
            ]
        );
    }

    #[test]
    fn a_cue_spanning_a_division_is_on_screen_in_both_fragments() {
        let subtitle = subtitle(&[(0, 6_000, "A")]);
        let fragments = fragment(&subtitle, &[2_000], 0);

        assert_eq!(
            (samples_of(&fragments[0]), samples_of(&fragments[1])),
            (vec![(0, 2_000, vec!["A"])], vec![(2_000, 6_000, vec!["A"])])
        );
    }

    #[test]
    fn merging_returns_the_cues_a_subtitle_was_authored_with() {
        let subtitle = subtitle(&[(0, 1_000, "A"), (2_000, 3_000, "B"), (4_000, 5_000, "C")]);

        assert_eq!(merge(&fragment(&subtitle, &[7_400], 2_000)), subtitle);
    }

    #[test]
    fn merging_recovers_a_cue_the_samples_split() {
        // "B" coming and going cuts "A" across three samples.
        let subtitle = subtitle(&[(0, 6_000, "A"), (1_000, 3_000, "B")]);

        assert_eq!(merge(&fragment(&subtitle, &[], 0)), subtitle);
    }

    #[test]
    fn merging_recovers_a_cue_that_spans_fragments() {
        let subtitle = subtitle(&[(0, 6_000, "A")]);

        assert_eq!(merge(&fragment(&subtitle, &[2_000], 2_000)), subtitle);
    }

    #[test]
    fn merging_keeps_cues_sharing_a_start_apart() {
        let subtitle = subtitle(&[(1_000, 2_000, "short"), (1_000, 4_000, "long")]);

        assert_eq!(merge(&fragment(&subtitle, &[], 0)), subtitle);
    }

    #[test]
    fn merging_cues_carrying_the_same_text_back_to_back_yields_one() {
        let authored = subtitle(&[(0, 1_000, "same"), (1_000, 2_000, "same")]);

        assert_eq!(
            merge(&fragment(&authored, &[], 0)),
            subtitle(&[(0, 2_000, "same")])
        );
    }

    #[test]
    fn merging_nothing_yields_a_subtitle_without_cues() {
        assert_eq!(merge(&[]), Subtitle::default());
    }

    #[test]
    fn a_fragment_no_cue_reaches_holds_one_empty_sample() {
        let subtitle = subtitle(&[(0, 1_000, "A"), (4_000, 5_000, "B")]);

        assert_eq!(
            samples_of(&fragment(&subtitle, &[], 2_000)[1]),
            [(2_000, 4_000, Vec::new())]
        );
    }

    #[test]
    fn durations_span_their_own_interval() {
        let subtitle = subtitle(&[(0, 3_000, "A")]);
        let fragments = fragment(&subtitle, &[2_000], 0);

        assert_eq!(
            (fragments[0].duration(), fragments[0].samples[0].duration()),
            (2_000, 2_000)
        );
    }

    fn subtitle(cues: &[(u32, u32, &str)]) -> Subtitle {
        Subtitle {
            cues: cues
                .iter()
                .map(|&(start, end, text)| Cue {
                    start,
                    end,
                    text: text.to_string(),
                })
                .collect(),
        }
    }

    fn spans(fragments: &[Fragment]) -> Vec<(u32, u32)> {
        fragments
            .iter()
            .map(|fragment| (fragment.start, fragment.end))
            .collect()
    }

    fn samples_of(fragment: &Fragment) -> Vec<(u32, u32, Vec<&str>)> {
        fragment
            .samples
            .iter()
            .map(|sample| {
                let texts = sample.cues.iter().map(|cue| cue.text.as_str()).collect();
                (sample.start, sample.end, texts)
            })
            .collect()
    }
}
