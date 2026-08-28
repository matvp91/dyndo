//! Timed-text models and format-specific parsers.

mod subtitle;
mod web_vtt_parser;

pub use subtitle::{Cue, Subtitle, SubtitleReadError};
pub use web_vtt_parser::{WebVttParseError, WebVttParser};
