//! The subtitle model every parser in this crate produces: a list of cues, each
//! holding a time span and its text.

/// One timed caption: `text`, shown over the half-open interval `[start, end)`.
///
/// Styling and positioning are deliberately absent. Every source format spells
/// them differently, and dyndo packages subtitles rather than renders them, so
/// the model keeps only what the formats agree on — when, and what.
///
/// Times are milliseconds, and a text track's timescale is always 1000, so they
/// are the media times a packager writes rather than something to be converted.
/// Their `u32` runs out after 49 days, which a parser is expected to reject
/// rather than leave for a packager to discover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    /// Presentation start, in milliseconds from the start of the timeline.
    pub start: u32,
    /// Presentation end, in milliseconds. Never precedes `start`.
    pub end: u32,
    /// The caption text. Multi-line captions keep their `\n` separators.
    pub text: String,
}

impl Cue {
    /// Whether two cues carry the same caption, ignoring when it is on screen.
    ///
    /// A [`Sample`](crate::fragmenter::Sample) records what is on screen, not how
    /// long for, so this is what decides whether a cue continues into the next one.
    /// Every field but the span belongs here: a caption differing only in how it is
    /// presented is a different caption.
    pub fn same_content(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

/// A parsed subtitle: the cues of a single text track, in presentation order
/// (by `start`, then `end`).
///
/// Cues may overlap — two captions on screen at once is ordinary — so this is a
/// list of what was authored, not a gapless timeline. Reconciling overlaps and
/// splitting the cues into segments belongs to whatever packages them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Subtitle {
    /// The cues, in presentation order.
    pub cues: Vec<Cue>,
}
