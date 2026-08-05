//! VTT parsing: a document in, a [`Subtitle`] out.

use crate::subtitle::{Cue, Subtitle};

const TIMING_ARROW: &str = "-->";

const NON_CUE_KEYWORDS: [&str; 3] = ["NOTE", "STYLE", "REGION"];

#[derive(Debug, thiserror::Error)]
pub enum VttError {
    #[error("missing WEBVTT signature")]
    MissingSignature,
    #[error("malformed timestamp {0:?}")]
    MalformedTimestamp(String),
    #[error("cue at {0}ms ends before it starts")]
    NegativeDuration(u64),
}

/// Parse a VTT document into a [`Subtitle`].
///
/// Everything VTT carries beyond timing and text is discarded: cue identifiers,
/// cue settings, `STYLE` and `REGION` blocks, `NOTE` comments, and the header's
/// metadata. Cue text is kept verbatim, inline markup included.
///
/// # Errors
///
/// [`VttError`] if the `WEBVTT` signature is absent, or if a cue's timing line
/// does not parse. A document that would otherwise drop captions is rejected
/// outright instead of yielding a subtitle that is silently short.
pub fn parse(document: &str) -> Result<Subtitle, VttError> {
    let document = document.strip_prefix('\u{feff}').unwrap_or(document);
    let mut lines = document.lines();

    if !is_signature(lines.next().unwrap_or_default()) {
        return Err(VttError::MissingSignature);
    }

    let mut cues = Vec::new();
    let mut block: Vec<&str> = Vec::new();
    // The appended blank line closes the last block.
    for line in lines.chain(std::iter::once("")) {
        if line.trim().is_empty() {
            cues.extend(parse_block(&block)?);
            block.clear();
        } else {
            block.push(line);
        }
    }

    cues.sort_by_key(|cue| (cue.start_ms, cue.end_ms));

    Ok(Subtitle { cues })
}

fn is_signature(line: &str) -> bool {
    line.strip_prefix("WEBVTT")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
}

fn parse_block(block: &[&str]) -> Result<Option<Cue>, VttError> {
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
    let (start_ms, end_ms) = parse_timing(timing)?;

    Ok(Some(Cue {
        start_ms,
        end_ms,
        text: text.join("\n"),
    }))
}

fn parse_timing(line: &str) -> Result<(u64, u64), VttError> {
    let (start, rest) = line
        .split_once(TIMING_ARROW)
        .expect("callers only pass timing lines");
    let start_ms = parse_timestamp(start.trim())?;
    let end_ms = parse_timestamp(rest.split_whitespace().next().unwrap_or_default())?;

    if end_ms < start_ms {
        return Err(VttError::NegativeDuration(start_ms));
    }

    Ok((start_ms, end_ms))
}

/// Accepts `HH:MM:SS.mmm` and `MM:SS.mmm`.
fn parse_timestamp(timestamp: &str) -> Result<u64, VttError> {
    let malformed = || VttError::MalformedTimestamp(timestamp.to_string());

    let (clock, millis) = timestamp.split_once('.').ok_or_else(malformed)?;
    if millis.len() != 3 {
        return Err(malformed());
    }
    let millis: u64 = millis.parse().map_err(|_| malformed())?;

    // Reading right to left lets the optional hours field fall out the end.
    let mut fields = clock.rsplit(':');
    let seconds = fields.next().unwrap_or_default();
    let minutes = fields.next().unwrap_or_default();
    let hours = fields.next().unwrap_or("0");
    if fields.next().is_some() {
        return Err(malformed());
    }

    let hours: u64 = hours.parse().map_err(|_| malformed())?;
    let minutes: u64 = minutes.parse().map_err(|_| malformed())?;
    let seconds: u64 = seconds.parse().map_err(|_| malformed())?;
    if minutes > 59 || seconds > 59 {
        return Err(malformed());
    }

    // Only the hours can overflow; the bounds above cap the other fields.
    hours
        .checked_mul(3_600_000)
        .and_then(|hours_ms| hours_ms.checked_add(minutes * 60_000 + seconds * 1_000 + millis))
        .ok_or_else(malformed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_single_cue() {
        let subtitle = parse("WEBVTT\n\n00:00.000 --> 00:02.000\nHello").unwrap();

        assert_eq!(
            subtitle.cues,
            vec![Cue {
                start_ms: 0,
                end_ms: 2_000,
                text: "Hello".to_string(),
            }]
        );
    }

    #[test]
    fn parses_a_document_with_no_cues() {
        let subtitle = parse("WEBVTT\n").unwrap();

        assert!(subtitle.cues.is_empty());
    }

    #[test]
    fn accepts_a_signature_with_a_title() {
        let subtitle = parse("WEBVTT - Episode 1\n\n00:00.000 --> 00:01.000\nx").unwrap();

        assert_eq!(subtitle.cues.len(), 1);
    }

    #[test]
    fn rejects_a_missing_signature() {
        let error = parse("00:00.000 --> 00:01.000\nx").unwrap_err();

        assert!(matches!(error, VttError::MissingSignature));
    }

    #[test]
    fn rejects_a_signature_run_into_other_text() {
        let error = parse("WEBVTTISH\n\n00:00.000 --> 00:01.000\nx").unwrap_err();

        assert!(matches!(error, VttError::MissingSignature));
    }

    #[test]
    fn keeps_multiline_text_verbatim() {
        let subtitle = parse("WEBVTT\n\n00:00.000 --> 00:02.000\n<b>one</b>\ntwo").unwrap();

        assert_eq!(subtitle.cues[0].text, "<b>one</b>\ntwo");
    }

    #[test]
    fn discards_the_cue_identifier() {
        let subtitle = parse("WEBVTT\n\nintro\n00:00.000 --> 00:02.000\nHi").unwrap();

        assert_eq!(
            (subtitle.cues[0].start_ms, subtitle.cues[0].text.as_str()),
            (0, "Hi")
        );
    }

    #[test]
    fn discards_cue_settings() {
        let subtitle = parse("WEBVTT\n\n00:00.000 --> 00:02.000 align:start line:90%\nHi").unwrap();

        assert_eq!(
            (subtitle.cues[0].end_ms, subtitle.cues[0].text.as_str()),
            (2_000, "Hi")
        );
    }

    #[test]
    fn skips_note_style_and_region_blocks() {
        let document = concat!(
            "WEBVTT\n\n",
            "NOTE a comment\nspanning two lines\n\n",
            "STYLE\n::cue { color: yellow }\n\n",
            "REGION\nid:top width:40%\n\n",
            "00:00.000 --> 00:01.000\nx",
        );

        let subtitle = parse(document).unwrap();

        assert_eq!(subtitle.cues.len(), 1);
    }

    #[test]
    fn skips_header_metadata() {
        let subtitle =
            parse("WEBVTT\nKind: captions\nLanguage: en\n\n00:00.000 --> 00:01.000\nx").unwrap();

        assert_eq!(subtitle.cues.len(), 1);
    }

    #[test]
    fn sorts_cues_by_start_then_end() {
        let document = concat!(
            "WEBVTT\n\n",
            "00:05.000 --> 00:06.000\nlast\n\n",
            "00:01.000 --> 00:04.000\nsecond\n\n",
            "00:01.000 --> 00:02.000\nfirst",
        );

        let subtitle = parse(document).unwrap();

        let texts: Vec<&str> = subtitle.cues.iter().map(|cue| cue.text.as_str()).collect();
        assert_eq!(texts, ["first", "second", "last"]);
    }

    #[test]
    fn keeps_overlapping_cues_as_authored() {
        let document = concat!(
            "WEBVTT\n\n",
            "00:00.000 --> 00:03.000\nA\n\n",
            "00:02.000 --> 00:05.000\nB",
        );

        let subtitle = parse(document).unwrap();

        assert_eq!(
            subtitle
                .cues
                .iter()
                .map(|cue| (cue.start_ms, cue.end_ms))
                .collect::<Vec<_>>(),
            [(0, 3_000), (2_000, 5_000)]
        );
    }

    #[test]
    fn reads_crlf_endings_and_a_byte_order_mark() {
        let subtitle = parse("\u{feff}WEBVTT\r\n\r\n00:00.000 --> 00:01.000\r\nx\r\n").unwrap();

        assert_eq!(subtitle.cues[0].text, "x");
    }

    #[test]
    fn rejects_an_end_before_its_start() {
        let error = parse("WEBVTT\n\n00:05.000 --> 00:02.000\nx").unwrap_err();

        assert!(matches!(error, VttError::NegativeDuration(5_000)));
    }

    #[test]
    fn rejects_a_malformed_cue_rather_than_dropping_it() {
        let document = concat!(
            "WEBVTT\n\n",
            "00:00.000 --> 00:01.000\nfine\n\n",
            "00:01.000 --> broken\nx",
        );

        let error = parse(document).unwrap_err();

        assert!(matches!(error, VttError::MalformedTimestamp(_)));
    }

    #[test]
    fn parses_both_timestamp_forms() {
        assert_eq!(
            (
                parse_timestamp("01:02:03.456").unwrap(),
                parse_timestamp("02:03.456").unwrap()
            ),
            (3_723_456, 123_456)
        );
    }

    #[test]
    fn rejects_a_timestamp_without_milliseconds() {
        assert!(parse_timestamp("00:05").is_err());
    }

    #[test]
    fn rejects_milliseconds_that_are_not_three_digits() {
        assert!(parse_timestamp("00:05.5").is_err());
    }

    #[test]
    fn rejects_a_timestamp_without_minutes() {
        assert!(parse_timestamp("05.000").is_err());
    }

    #[test]
    fn rejects_a_timestamp_with_too_many_fields() {
        assert!(parse_timestamp("1:00:00:05.000").is_err());
    }

    #[test]
    fn rejects_out_of_range_minutes_and_seconds() {
        assert!(parse_timestamp("75:00.000").is_err() && parse_timestamp("00:75.000").is_err());
    }

    #[test]
    fn rejects_hours_that_overflow_milliseconds() {
        assert!(parse_timestamp("5124095576030431:00:00.000").is_err());
    }

    #[test]
    fn rejects_a_garbage_timestamp() {
        assert!(parse_timestamp("abc").is_err());
    }
}
