use opendal::Operator;
use relative_path::RelativePath;

use super::vtt;
use crate::next::error::Error;
use crate::next::format::Format;

/// Parsed format-independent timed-text content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedText {
    /// Cues in source order.
    pub cues: Vec<Cue>,
}

/// A timed text payload displayed over a presentation interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    /// Start time in milliseconds from the start of the presentation.
    pub start_ms: u64,
    /// End time in milliseconds from the start of the presentation.
    pub end_ms: u64,
    /// Cue text, with multiline payloads separated by newlines.
    pub text: String,
}

impl TimedText {
    /// Read timed-text content from `path`.
    ///
    /// # Errors
    /// Returns an error when the path is not WebVTT, storage cannot be read,
    /// the bytes are not UTF-8, or the WebVTT content is invalid.
    pub async fn read(op: &Operator, path: &RelativePath) -> Result<Self, Error> {
        if Format::from_path(path)? != Format::Vtt {
            return Err(parse_error(path, "the track is not a WebVTT file"));
        }

        let buf = op
            .read(path.as_str())
            .await
            .map_err(|source| Error::OpenTrack {
                path: path.to_owned(),
                source,
            })?;
        let bytes = buf.to_bytes();
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| parse_error(path, format!("the file is not UTF-8: {error}")))?;

        vtt::parse(text).map_err(|reason| parse_error(path, reason))
    }
}

fn parse_error(path: &RelativePath, reason: impl Into<String>) -> Error {
    Error::ParseText {
        path: path.to_owned(),
        reason: reason.into(),
    }
}
