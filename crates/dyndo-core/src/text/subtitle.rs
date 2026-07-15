//! The format-neutral subtitle model: [`Subtitle`] and [`Cue`].

/// A parsed subtitle cue (format-neutral).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    /// Start time, in milliseconds.
    pub start_ms: u64,
    /// End time, in milliseconds.
    pub end_ms: u64,
    /// Cue text, verbatim. An empty string denotes a gap-fill cue.
    pub text: String,
}

impl Cue {
    /// The cue's length, in milliseconds (`end_ms - start_ms`).
    pub fn duration(&self) -> u64 {
        self.end_ms - self.start_ms
    }
}

/// A subtitle track: a language plus its cues (in presentation order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subtitle {
    /// ISO-639-2 language code, written into the track's `mdhd` by
    /// [`crate::text::wvtt::pack`].
    pub language: String,
    /// The cues, sorted by `(start_ms, end_ms)`.
    pub cues: Vec<Cue>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_is_end_minus_start() {
        let cue = Cue {
            start_ms: 1000,
            end_ms: 3500,
            text: "hi".into(),
        };
        assert_eq!(cue.duration(), 2500);
    }
}
