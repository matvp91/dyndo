/// A timed-text cue with millisecond timestamps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    /// The inclusive start time in milliseconds.
    pub start: u32,
    /// The exclusive end time in milliseconds.
    pub end: u32,
    /// The cue content.
    pub text: String,
}

/// A collection of timed-text cues.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Subtitle {
    /// The subtitle cues.
    pub cues: Vec<Cue>,
}

impl Subtitle {
    /// Returns the timestamp of the latest cue end, in milliseconds.
    pub fn duration(&self) -> u32 {
        self.cues.iter().map(|cue| cue.end).max().unwrap_or(0)
    }

    /// Returns the portion of this subtitle that overlaps `start..end`.
    pub fn slice(&self, start: u32, end: u32) -> Option<Self> {
        if start >= end {
            return None;
        }

        let cues = self
            .cues
            .iter()
            .filter(|cue| cue.start < end && cue.end > start)
            .map(|cue| Cue {
                start: cue.start.max(start),
                end: cue.end.min(end),
                text: cue.text.clone(),
            })
            .collect();

        Some(Self { cues })
    }
}

#[cfg(test)]
mod tests {
    use super::{Cue, Subtitle};

    #[test]
    fn slice_clamps_overlapping_cues_to_its_range() {
        let subtitle = Subtitle {
            cues: vec![Cue {
                start: 500,
                end: 1_500,
                text: "cue".into(),
            }],
        };

        assert_eq!(
            subtitle.slice(1_000, 2_000),
            Some(Subtitle {
                cues: vec![Cue {
                    start: 1_000,
                    end: 1_500,
                    text: "cue".into(),
                }],
            })
        );
    }
}
