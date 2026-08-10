use std::iter::successors;
use std::ops::Range;

use super::{Cue, Subtitle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSegment<'a> {
    start: u32,
    samples: Vec<TextSample<'a>>,
}

impl<'a> TextSegment<'a> {
    pub fn start(&self) -> u32 {
        self.start
    }

    pub fn samples(&self) -> &[TextSample<'a>] {
        &self.samples
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSample<'a> {
    start: u32,
    end: u32,
    cues: Vec<&'a Cue>,
}

impl<'a> TextSample<'a> {
    pub fn start(&self) -> u32 {
        self.start
    }

    pub fn end(&self) -> u32 {
        self.end
    }

    pub fn duration(&self) -> u32 {
        self.end - self.start
    }

    pub fn cues(&self) -> &[&'a Cue] {
        &self.cues
    }
}

impl Subtitle {
    pub fn segments(&self, duration: u32, boundaries: &[u32]) -> Vec<TextSegment<'_>> {
        let divisions = boundaries
            .iter()
            .copied()
            .chain(grid(duration, self.duration()));

        partition(0..self.duration(), divisions)
            .into_iter()
            .map(|range| TextSegment {
                start: range.start,
                samples: self.samples(range),
            })
            .collect()
    }

    fn samples(&self, range: Range<u32>) -> Vec<TextSample<'_>> {
        if range.is_empty() {
            return Vec::new();
        }

        let cues: Vec<_> = self
            .cues
            .iter()
            .filter(|cue| cue.start < range.end && cue.end > range.start)
            .collect();
        let edges = cues.iter().flat_map(|cue| [cue.start, cue.end]);

        partition(range, edges)
            .into_iter()
            .map(|range| TextSample {
                start: range.start,
                end: range.end,
                cues: cues
                    .iter()
                    .copied()
                    .filter(|cue| cue.start <= range.start && cue.end > range.start)
                    .collect(),
            })
            .collect()
    }
}

fn partition(range: Range<u32>, cuts: impl IntoIterator<Item = u32>) -> Vec<Range<u32>> {
    let mut edges: Vec<_> = cuts
        .into_iter()
        .filter(|&cut| cut > range.start && cut < range.end)
        .chain([range.start, range.end])
        .collect();
    edges.sort_unstable();
    edges.dedup();

    edges.windows(2).map(|edges| edges[0]..edges[1]).collect()
}

fn grid(length: u32, end: u32) -> impl Iterator<Item = u32> {
    successors((length > 0).then_some(length), move |time| {
        time.checked_add(length)
    })
    .take_while(move |&time| time < end)
}

#[cfg(test)]
mod tests {
    use super::Subtitle;
    use crate::text::Cue;

    fn subtitle() -> Subtitle {
        Subtitle {
            cues: vec![
                Cue {
                    start: 500,
                    end: 1_500,
                    text: "first".into(),
                },
                Cue {
                    start: 1_000,
                    end: 2_500,
                    text: "second".into(),
                },
            ],
        }
    }

    #[test]
    fn segments_splits_samples_when_active_cues_change() {
        let subtitle = subtitle();

        let segments = subtitle.segments(2_000, &[]);

        assert_eq!(
            segments[0]
                .samples()
                .iter()
                .map(|sample| (sample.start(), sample.end(), sample.cues().len()))
                .collect::<Vec<_>>(),
            [
                (0, 500, 0),
                (500, 1_000, 1),
                (1_000, 1_500, 2),
                (1_500, 2_000, 1)
            ]
        );
    }

    #[test]
    fn segments_uses_boundaries_in_addition_to_the_duration_grid() {
        let subtitle = subtitle();

        let segments = subtitle.segments(2_000, &[750, 750, 3_000]);

        assert_eq!(
            segments
                .iter()
                .map(|segment| segment.start())
                .collect::<Vec<_>>(),
            [0, 750, 2_000]
        );
    }

    #[test]
    fn segments_with_zero_duration_only_uses_boundaries() {
        let subtitle = subtitle();

        let segments = subtitle.segments(0, &[1_000]);

        assert_eq!(
            segments
                .iter()
                .map(|segment| segment.start())
                .collect::<Vec<_>>(),
            [0, 1_000]
        );
    }

    #[test]
    fn segments_excludes_a_cue_from_the_sample_at_its_end() {
        let subtitle = Subtitle {
            cues: vec![Cue {
                start: 0,
                end: 1_000,
                text: "cue".into(),
            }],
        };

        let segments = subtitle.segments(2_000, &[]);

        assert_eq!(segments[0].samples().last().unwrap().cues().len(), 1);
    }

    #[test]
    fn segments_returns_no_segments_for_an_empty_subtitle() {
        assert!(Subtitle::default().segments(1_000, &[]).is_empty());
    }
}
