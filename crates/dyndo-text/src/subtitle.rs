//! The subtitle model every parser in this crate produces: a list of cues, each
//! holding a time span and its text.

/// One timed caption: `text`, shown over the half-open interval `[start, end)`.
///
/// Styling and positioning are deliberately absent. Every source format spells
/// them differently, and dyndo packages subtitles rather than renders them, so
/// the model keeps only what the formats agree on — when, and what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    /// Presentation start, in milliseconds from the start of the timeline.
    pub start: u64,
    /// Presentation end, in milliseconds. Never precedes `start`.
    pub end: u64,
    /// The caption text. Multi-line captions keep their `\n` separators.
    pub text: String,
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
