use std::ops::Range;

use super::{Cue, Subtitle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineSample<'a> {
    start: u32,
    end: u32,
    cues: Vec<&'a Cue>,
}

impl<'a> TimelineSample<'a> {
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

pub fn samples(subtitle: &Subtitle, range: Range<u32>) -> Vec<TimelineSample<'_>> {
    if range.is_empty() {
        return Vec::new();
    }

    let cues: Vec<_> = subtitle
        .cues
        .iter()
        .filter(|cue| cue.start < range.end && cue.end > range.start)
        .collect();
    let edges = cues.iter().flat_map(|cue| [cue.start, cue.end]);

    intervals(range, edges)
        .into_iter()
        .map(|range| TimelineSample {
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

fn intervals(range: Range<u32>, divisions: impl IntoIterator<Item = u32>) -> Vec<Range<u32>> {
    let mut edges: Vec<_> = divisions
        .into_iter()
        .filter(|&division| division > range.start && division < range.end)
        .chain([range.start, range.end])
        .collect();
    edges.sort_unstable();
    edges.dedup();

    edges.windows(2).map(|edges| edges[0]..edges[1]).collect()
}
