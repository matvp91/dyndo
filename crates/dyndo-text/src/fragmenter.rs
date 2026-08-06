//! Dividing a [`Subtitle`] into the fragments a CMAF text track is built from.
//!
//! Every time here is a millisecond, as on [`Cue`].

use std::iter::successors;

use crate::subtitle::{Cue, Subtitle};

/// The cues on screen over `[start, end)`, in presentation order, or none where
/// nothing is.
///
/// The cues are the ones the subtitle was authored with, so one that began before
/// the sample or outlasts it keeps its own span. No cue edge falls inside a
/// sample, so each of them is on screen for all of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample<'a> {
    pub start: u64,
    pub end: u64,
    pub cues: Vec<&'a Cue>,
}

impl Sample<'_> {
    pub fn duration(&self) -> u64 {
        self.end - self.start
    }
}

/// Consecutive samples covering `[start, end)` without holes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment<'a> {
    pub start: u64,
    pub end: u64,
    pub samples: Vec<Sample<'a>>,
}

impl Fragment<'_> {
    pub fn duration(&self) -> u64 {
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
pub fn fragment<'a>(subtitle: &'a Subtitle, boundaries: &[u64], length: u64) -> Vec<Fragment<'a>> {
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
fn samples<'a>(subtitle: &'a Subtitle, start: u64, end: u64) -> Vec<Sample<'a>> {
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
                .copied()
                .filter(|cue| cue.start <= start && cue.end > start)
                .collect(),
        })
        .collect()
}

/// Every multiple of `length` below `end`. A `length` of 0 asks for no grid.
fn grid(length: u64, end: u64) -> impl Iterator<Item = u64> {
    successors((length > 0).then_some(length), move |time| {
        time.checked_add(length)
    })
    .take_while(move |&time| time < end)
}

/// The consecutive intervals `[start, end)` falls into when divided at each time
/// in `at`. A time outside it names no interval, and a repeated one divides once,
/// so both are ignored.
fn intervals(start: u64, end: u64, at: impl IntoIterator<Item = u64>) -> Vec<(u64, u64)> {
    let mut times: Vec<u64> = at
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
    fn a_cue_keeps_its_own_span_inside_a_sample() {
        let subtitle = subtitle(&[(0, 6_000, "A")]);
        let fragments = fragment(&subtitle, &[2_000], 0);

        let cue = fragments[1].samples[0].cues[0];
        assert_eq!((cue.start, cue.end), (0, 6_000));
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

    fn subtitle(cues: &[(u64, u64, &str)]) -> Subtitle {
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

    fn spans(fragments: &[Fragment]) -> Vec<(u64, u64)> {
        fragments
            .iter()
            .map(|fragment| (fragment.start, fragment.end))
            .collect()
    }

    fn samples_of<'a>(fragment: &'a Fragment) -> Vec<(u64, u64, Vec<&'a str>)> {
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
