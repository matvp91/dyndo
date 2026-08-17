use std::time::Duration;

use super::{Cue, Subtitle};

const TIMING_ARROW: &str = "-->";
const NON_CUE_KEYWORDS: [&str; 3] = ["NOTE", "STYLE", "REGION"];

/// Parses WebVTT documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebVttParser;

/// An error parsing a WebVTT document.
#[derive(Debug, thiserror::Error)]
pub enum WebVttParseError {
    /// The document did not begin with the `WEBVTT` signature.
    #[error("missing WEBVTT signature")]
    MissingSignature,
    /// A timestamp did not have a valid WebVTT representation.
    #[error("malformed timestamp {0:?}")]
    MalformedTimestamp(String),
    /// A cue ended before it started.
    #[error("cue at {0:?} ends before it starts")]
    NegativeDuration(Duration),
}

impl WebVttParser {
    /// Parses a WebVTT document into a [`Subtitle`].
    ///
    /// # Errors
    ///
    /// Returns [`WebVttParseError`] if the document is invalid.
    pub fn parse(document: &str) -> Result<Subtitle, WebVttParseError> {
        let document = document.strip_prefix('\u{feff}').unwrap_or(document);
        let mut lines = document.lines();

        if !is_signature(lines.next().unwrap_or_default()) {
            return Err(WebVttParseError::MissingSignature);
        }

        let mut cues = Vec::new();
        let mut block = Vec::new();
        for line in lines.chain(std::iter::once("")) {
            if line.trim().is_empty() {
                cues.extend(parse_block(&block)?);
                block.clear();
            } else {
                block.push(line);
            }
        }

        cues.sort_by_key(|cue| (cue.start, cue.end));

        Ok(Subtitle { cues })
    }
}

fn is_signature(line: &str) -> bool {
    line.strip_prefix("WEBVTT")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
}

fn parse_block(block: &[&str]) -> Result<Option<Cue>, WebVttParseError> {
    let keyword = block
        .first()
        .and_then(|line| line.split_whitespace().next());
    if keyword.is_some_and(|keyword| NON_CUE_KEYWORDS.contains(&keyword)) {
        return Ok(None);
    }

    let (timing, text) = match block {
        [timing, text @ ..] if timing.contains(TIMING_ARROW) => (timing, text),
        [_identifier, timing, text @ ..] if timing.contains(TIMING_ARROW) => (timing, text),
        _ => return Ok(None),
    };
    let (start, end) = parse_timing(timing)?;

    Ok(Some(Cue {
        start,
        end,
        text: text.join("\n"),
    }))
}

fn parse_timing(line: &str) -> Result<(Duration, Duration), WebVttParseError> {
    let (start, end) = line
        .split_once(TIMING_ARROW)
        .expect("callers only pass timing lines");
    let start = parse_timestamp(start.trim())?;
    let end = parse_timestamp(end.split_whitespace().next().unwrap_or_default())?;

    if end < start {
        return Err(WebVttParseError::NegativeDuration(start));
    }

    Ok((start, end))
}

fn parse_timestamp(timestamp: &str) -> Result<Duration, WebVttParseError> {
    let malformed = || WebVttParseError::MalformedTimestamp(timestamp.to_string());

    let (clock, millis) = timestamp.split_once('.').ok_or_else(malformed)?;
    if millis.len() != 3 {
        return Err(malformed());
    }
    let millis: u32 = millis.parse().map_err(|_| malformed())?;

    let mut fields = clock.rsplit(':');
    let seconds = fields.next().unwrap_or_default();
    let minutes = fields.next().unwrap_or_default();
    let hours = fields.next().unwrap_or("0");
    if fields.next().is_some() {
        return Err(malformed());
    }

    let hours: u32 = hours.parse().map_err(|_| malformed())?;
    let minutes: u32 = minutes.parse().map_err(|_| malformed())?;
    let seconds: u32 = seconds.parse().map_err(|_| malformed())?;
    if minutes > 59 || seconds > 59 {
        return Err(malformed());
    }

    let milliseconds = u64::from(hours)
        .checked_mul(3_600_000)
        .and_then(|hours| {
            hours.checked_add(
                u64::from(minutes) * 60_000 + u64::from(seconds) * 1_000 + u64::from(millis),
            )
        })
        .ok_or_else(malformed)?;

    Ok(Duration::from_millis(milliseconds))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Cue, WebVttParseError, WebVttParser};

    #[test]
    fn parse_sorts_cues_and_preserves_multiline_text() {
        let subtitle = WebVttParser::parse(
            "WEBVTT\n\nsecond\n00:00:02.000 --> 00:00:03.000 align:start\nTwo\n\n00:00:00.500 --> 00:00:01.000\nOne\nline two\n",
        )
        .unwrap();

        assert_eq!(
            subtitle.cues,
            vec![
                Cue {
                    start: Duration::from_millis(500),
                    end: Duration::from_millis(1_000),
                    text: "One\nline two".into(),
                },
                Cue {
                    start: Duration::from_millis(2_000),
                    end: Duration::from_millis(3_000),
                    text: "Two".into(),
                },
            ]
        );
    }

    #[test]
    fn parse_ignores_non_cue_blocks() {
        let subtitle = WebVttParser::parse(
            "WEBVTT\n\nNOTE ignored\nmetadata\n\nSTYLE\n::cue { color: lime; }\n\n00:00:00.000 --> 00:00:01.000\nKept\n",
        )
        .unwrap();

        assert_eq!(
            subtitle.cues,
            vec![Cue {
                start: Duration::ZERO,
                end: Duration::from_millis(1_000),
                text: "Kept".into(),
            }]
        );
    }

    #[test]
    fn parse_rejects_a_missing_signature() {
        let error = WebVttParser::parse("00:00:00.000 --> 00:00:01.000\nText\n").unwrap_err();

        assert!(matches!(error, WebVttParseError::MissingSignature));
    }

    #[test]
    fn parse_rejects_timestamps_with_invalid_seconds() {
        let error =
            WebVttParser::parse("WEBVTT\n\n00:00:60.000 --> 00:01:01.000\nText\n").unwrap_err();

        assert!(matches!(error, WebVttParseError::MalformedTimestamp(_)));
    }

    #[test]
    fn parse_rejects_a_cue_that_ends_before_it_starts() {
        let error =
            WebVttParser::parse("WEBVTT\n\n00:00:02.000 --> 00:00:01.000\nText\n").unwrap_err();

        assert!(matches!(
            error,
            WebVttParseError::NegativeDuration(duration) if duration == Duration::from_secs(2)
        ));
    }
}
