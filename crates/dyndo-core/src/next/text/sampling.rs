//! Text-specific sampling inside media ranges chosen by segmentation policy.

use std::ops::Range;

use super::super::segmentation::partition;
use super::{Cue, Subtitle};

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
    /// Splits a media range wherever its active set of cues changes.
    pub fn samples(&self, range: Range<u32>) -> Vec<TextSample<'_>> {
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
