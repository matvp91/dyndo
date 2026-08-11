use super::{Cue, Subtitle};

const TIMING_ARROW: &str = "-->";

const NON_CUE_KEYWORDS: [&str; 3] = ["NOTE", "STYLE", "REGION"];

#[derive(Debug, thiserror::Error)]
pub enum WebVttParseError {
    #[error("missing WEBVTT signature")]
    MissingSignature,
    #[error("malformed timestamp {0:?}")]
    MalformedTimestamp(String),
    #[error("cue at {0}ms ends before it starts")]
    NegativeDuration(u32),
}

impl Subtitle {
    pub fn from_vtt_text(document: &str) -> Result<Self, WebVttParseError> {
        let document = document.strip_prefix('\u{feff}').unwrap_or(document);
        let mut lines = document.lines();

        if !is_signature(lines.next().unwrap_or_default()) {
            return Err(WebVttParseError::MissingSignature);
        }

        let mut cues = Vec::new();
        let mut block: Vec<&str> = Vec::new();
        for line in lines.chain(std::iter::once("")) {
            if line.trim().is_empty() {
                cues.extend(parse_block(&block)?);
                block.clear();
            } else {
                block.push(line);
            }
        }

        cues.sort_by_key(|cue| (cue.start, cue.end));

        Ok(Self { cues })
    }

    pub fn to_vtt_text(&self) -> String {
        let mut document = String::from("WEBVTT\n");
        for cue in &self.cues {
            document.push('\n');
            document.push_str(&write_timestamp(cue.start));
            document.push_str(" --> ");
            document.push_str(&write_timestamp(cue.end));
            document.push('\n');
            document.push_str(&cue.text);
            document.push('\n');
        }

        document
    }
}

fn write_timestamp(timestamp: u32) -> String {
    let millis = timestamp % 1_000;
    let seconds = timestamp / 1_000;

    format!(
        "{:02}:{:02}:{:02}.{millis:03}",
        seconds / 3_600,
        seconds / 60 % 60,
        seconds % 60
    )
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

fn parse_timing(line: &str) -> Result<(u32, u32), WebVttParseError> {
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

fn parse_timestamp(timestamp: &str) -> Result<u32, WebVttParseError> {
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

    hours
        .checked_mul(3_600_000)
        .and_then(|hours| hours.checked_add(minutes * 60_000 + seconds * 1_000 + millis))
        .ok_or_else(malformed)
}

#[cfg(test)]
mod tests {
    use super::{Cue, Subtitle, WebVttParseError};

    #[test]
    fn from_vtt_text_sorts_cues_and_preserves_multiline_text() {
        let subtitle = Subtitle::from_vtt_text(
            "WEBVTT\n\nsecond\n00:00:02.000 --> 00:00:03.000 align:start\nTwo\n\n00:00:00.500 --> 00:00:01.000\nOne\nline two\n",
        )
        .unwrap();

        assert_eq!(
            subtitle.cues,
            vec![
                Cue {
                    start: 500,
                    end: 1_000,
                    text: "One\nline two".into()
                },
                Cue {
                    start: 2_000,
                    end: 3_000,
                    text: "Two".into()
                }
            ]
        );
    }

    #[test]
    fn from_vtt_text_ignores_non_cue_blocks() {
        let subtitle = Subtitle::from_vtt_text(
            "WEBVTT\n\nNOTE ignored\nmetadata\n\nSTYLE\n::cue { color: lime; }\n\n00:00:00.000 --> 00:00:01.000\nKept\n",
        )
        .unwrap();

        assert_eq!(
            subtitle.cues,
            vec![Cue {
                start: 0,
                end: 1_000,
                text: "Kept".into()
            }]
        );
    }

    #[test]
    fn from_vtt_text_rejects_a_missing_signature() {
        let error = Subtitle::from_vtt_text("00:00:00.000 --> 00:00:01.000\nText\n").unwrap_err();

        assert!(matches!(error, WebVttParseError::MissingSignature));
    }

    #[test]
    fn from_vtt_text_rejects_timestamps_with_invalid_seconds() {
        let error =
            Subtitle::from_vtt_text("WEBVTT\n\n00:00:60.000 --> 00:01:01.000\nText\n").unwrap_err();

        assert!(matches!(error, WebVttParseError::MalformedTimestamp(_)));
    }

    #[test]
    fn from_vtt_text_rejects_a_cue_that_ends_before_it_starts() {
        let error =
            Subtitle::from_vtt_text("WEBVTT\n\n00:00:02.000 --> 00:00:01.000\nText\n").unwrap_err();

        assert!(matches!(error, WebVttParseError::NegativeDuration(2_000)));
    }

    #[test]
    fn to_vtt_text_formats_hour_boundaries() {
        let subtitle = Subtitle {
            cues: vec![Cue {
                start: 3_600_000,
                end: 3_601_001,
                text: "Text".into(),
            }],
        };

        assert_eq!(
            subtitle.to_vtt_text(),
            "WEBVTT\n\n01:00:00.000 --> 01:00:01.001\nText\n"
        );
    }
}
