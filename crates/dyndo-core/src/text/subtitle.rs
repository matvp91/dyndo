use std::time::Duration;

use relative_path::RelativePath;
use thiserror::Error;

use super::{WebVttParseError, WebVttParser};
use crate::storage::{Storage, StorageError};

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

/// Errors returned while loading a subtitle source.
#[derive(Debug, Error)]
pub enum SubtitleReadError {
    /// The path does not identify a supported subtitle format.
    #[error("unsupported subtitle format at {0}")]
    UnsupportedFormat(String),
    /// Source storage was unavailable.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// The subtitle source could not be read.
    #[error("failed to read subtitle source: {0}")]
    Source(#[from] opendal::Error),
    /// The subtitle source was not valid UTF-8.
    #[error("subtitle source is not valid UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    /// The WebVTT document was invalid.
    #[error(transparent)]
    WebVtt(#[from] WebVttParseError),
}

impl Subtitle {
    /// Loads a subtitle from source storage based on its file extension.
    ///
    /// # Errors
    ///
    /// Returns [`SubtitleReadError`] when the format is unsupported, storage
    /// cannot be read, the source is not UTF-8, or the subtitle is invalid.
    pub async fn from_path(path: &RelativePath) -> Result<Self, SubtitleReadError> {
        if path.extension() != Some("vtt") {
            return Err(SubtitleReadError::UnsupportedFormat(path.to_string()));
        }

        let bytes = Storage::source_op()?.read(path.as_str()).await?.to_bytes();
        let document = std::str::from_utf8(&bytes)?;

        Ok(WebVttParser::parse(document)?)
    }

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
