//! Format-independent timed text.

use opendal::Operator;
use relative_path::RelativePath;

/// Parsed format-independent timed-text content.
///
/// Cue, region, and on-demand rendering support will be added later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedText;

impl TimedText {
    /// Read timed-text content from `path`.
    ///
    /// # Errors
    /// Always returns [`TimedTextNotImplemented`] until timed-text parsing is
    /// implemented.
    pub async fn read(
        _op: &Operator,
        _path: &RelativePath,
    ) -> Result<Self, TimedTextNotImplemented> {
        Err(TimedTextNotImplemented)
    }
}

/// Timed-text parsing has not been implemented yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("timed-text parsing is not implemented")]
pub struct TimedTextNotImplemented;
