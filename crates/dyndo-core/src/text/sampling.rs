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
