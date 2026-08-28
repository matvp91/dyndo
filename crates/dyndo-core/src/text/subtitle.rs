use std::time::Duration;

/// A timed-text cue with presentation timestamps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    /// The inclusive start time.
    pub start: Duration,
    /// The exclusive end time.
    pub end: Duration,
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
    /// Returns the timestamp of the latest cue end.
    pub fn duration(&self) -> Duration {
        self.cues
            .iter()
            .map(|cue| cue.end)
            .max()
            .unwrap_or_default()
    }

    /// Returns the portion of this subtitle that overlaps `start..end`.
    pub fn slice(&self, start: Duration, end: Duration) -> Option<Self> {
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
    use std::time::Duration;

    use super::{Cue, Subtitle};

    fn duration(milliseconds: u64) -> Duration {
        Duration::from_millis(milliseconds)
    }

    #[test]
    fn slice_clamps_overlapping_cues_to_its_range() {
        let subtitle = Subtitle {
            cues: vec![Cue {
                start: duration(500),
                end: duration(1_500),
                text: "cue".into(),
            }],
        };

        assert_eq!(
            subtitle.slice(duration(1_000), duration(2_000)),
            Some(Subtitle {
                cues: vec![Cue {
                    start: duration(1_000),
                    end: duration(1_500),
                    text: "cue".into(),
                }],
            })
        );
    }
}
