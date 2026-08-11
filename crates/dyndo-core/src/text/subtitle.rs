#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    pub start: u32,
    pub end: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Subtitle {
    pub cues: Vec<Cue>,
}

impl Subtitle {
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
