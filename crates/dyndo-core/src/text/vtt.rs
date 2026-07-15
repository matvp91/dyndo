//! WebVTT document parsing: `&str` → [`Subtitle`].

use std::borrow::Cow;

use super::error::CoreTextError;
use super::subtitle::{Cue, Subtitle};

/// Parse a WebVTT document into a [`Subtitle`].
///
/// Recognizes the `WEBVTT` signature, cue timing lines, and multi-line payloads;
/// the optional cue id line and any cue settings are recognized but discarded,
/// as are header/`STYLE`/`REGION`/`NOTE` blocks. `language` is initialized to
/// `"und"`.
///
/// # Errors
/// [`CoreTextError::Vtt`] if the signature is missing, a timestamp is malformed,
/// or a cue's end precedes its start.
pub fn parse(input: &str) -> Result<Subtitle, CoreTextError> {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    // Normalize line endings to `\n` lazily: the common LF-only document is
    // borrowed as-is; only CR-bearing input pays for an owned copy.
    let normalized: Cow<'_, str> = if input.contains('\r') {
        Cow::Owned(input.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(input)
    };

    if !normalized
        .lines()
        .next()
        .unwrap_or("")
        .starts_with("WEBVTT")
    {
        return Err(CoreTextError::Vtt("missing WEBVTT signature".into()));
    }

    let mut cues = Vec::new();
    for block in split_blocks(&normalized) {
        if is_cue(block) {
            cues.push(parse_cue(block)?);
        }
        // Non-cue blocks (header, STYLE/REGION, NOTE) are ignored.
    }
    cues.sort_by_key(|c| (c.start_ms, c.end_ms));

    Ok(Subtitle {
        language: "und".to_string(),
        cues,
    })
}

/// Split into blocks separated by one or more blank lines. Blocks are
/// subslices of `s` (which is `\n`-normalized), so no text is copied; a
/// block spans its first through last non-blank line, interior newlines
/// included.
fn split_blocks(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut end = 0;
    let mut pos = 0;
    for line in s.split('\n') {
        if line.trim().is_empty() {
            if let Some(start) = start.take() {
                out.push(&s[start..end]);
            }
        } else {
            start.get_or_insert(pos);
            end = pos + line.len();
        }
        pos += line.len() + 1;
    }
    if let Some(start) = start {
        out.push(&s[start..end]);
    }
    out
}

/// A block is a cue if it is not a `NOTE` comment and carries a `-->` line.
fn is_cue(block: &str) -> bool {
    !block.starts_with("NOTE") && block.lines().any(|l| l.contains("-->"))
}

fn parse_cue(block: &str) -> Result<Cue, CoreTextError> {
    let (first, rest) = block.split_once('\n').unwrap_or((block, ""));
    let (timing, text) = if first.contains("-->") {
        (first, rest)
    } else if rest.is_empty() {
        return Err(CoreTextError::Vtt("cue id without a timing line".into()));
    } else {
        // First line is the cue id — recognized but discarded.
        rest.split_once('\n').unwrap_or((rest, ""))
    };

    let (start_raw, rest) = timing
        .split_once("-->")
        .ok_or_else(|| CoreTextError::Vtt("timing line has no '-->'".into()))?;
    let start_ms = parse_timestamp(start_raw.trim())?;

    // The end timestamp is the first token after `-->`; trailing settings dropped.
    let end_token = rest.split_whitespace().next().unwrap_or("");
    let end_ms = parse_timestamp(end_token)?;

    if end_ms < start_ms {
        return Err(CoreTextError::Vtt(format!(
            "cue end {end_ms} precedes start {start_ms}"
        )));
    }

    // `block` is a contiguous slice, so the payload after the timing line is
    // already the joined multi-line text — one allocation, no collect/join.
    Ok(Cue {
        start_ms,
        end_ms,
        text: text.to_string(),
    })
}

/// Parse a WebVTT timestamp (`HH:MM:SS.mmm` or `MM:SS.mmm`) into milliseconds.
fn parse_timestamp(s: &str) -> Result<u64, CoreTextError> {
    let err = || CoreTextError::Vtt(format!("invalid timestamp {s:?}"));
    let (hms, millis) = s.split_once('.').ok_or_else(err)?;
    if millis.len() != 3 {
        return Err(err());
    }
    let ms: u64 = millis.parse().map_err(|_| err())?;
    let parts: Vec<&str> = hms.split(':').collect();
    let (h, m, sec) = match parts.as_slice() {
        [h, m, sec] => (
            h.parse::<u64>().map_err(|_| err())?,
            m.parse::<u64>().map_err(|_| err())?,
            sec.parse::<u64>().map_err(|_| err())?,
        ),
        [m, sec] => (
            0,
            m.parse::<u64>().map_err(|_| err())?,
            sec.parse::<u64>().map_err(|_| err())?,
        ),
        _ => return Err(err()),
    };
    if m >= 60 || sec >= 60 {
        return Err(err());
    }
    Ok((h * 3600 + m * 60 + sec) * 1000 + ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_signature() {
        assert!(parse("NOTVTT\n\n00:00.000 --> 00:01.000\nx").is_err());
    }

    #[test]
    fn parses_a_single_cue() {
        let s = parse("WEBVTT\n\n00:00.000 --> 00:02.000\nHello").unwrap();
        assert_eq!(s.language, "und");
        assert_eq!(s.cues.len(), 1);
        assert_eq!(s.cues[0].start_ms, 0);
        assert_eq!(s.cues[0].end_ms, 2000);
        assert_eq!(s.cues[0].text, "Hello");
    }

    #[test]
    fn ignores_cue_id_and_settings() {
        let s = parse("WEBVTT\n\nintro\n00:00.000 --> 00:02.000 align:start line:90%\nHi").unwrap();
        assert_eq!(s.cues.len(), 1);
        assert_eq!(s.cues[0].start_ms, 0);
        assert_eq!(s.cues[0].end_ms, 2000);
        assert_eq!(s.cues[0].text, "Hi");
    }

    #[test]
    fn joins_multiline_text() {
        let s = parse("WEBVTT\n\n00:00.000 --> 00:02.000\nline one\nline two").unwrap();
        assert_eq!(s.cues[0].text, "line one\nline two");
    }

    #[test]
    fn ignores_style_and_note_blocks() {
        let doc =
            "WEBVTT\n\nSTYLE\n::cue { color: yellow }\n\nNOTE hello\n\n00:00.000 --> 00:01.000\nx";
        let s = parse(doc).unwrap();
        assert_eq!(s.cues.len(), 1);
        assert_eq!(s.cues[0].text, "x");
    }

    #[test]
    fn normalizes_crlf_and_bom() {
        let doc = "\u{feff}WEBVTT\r\n\r\n00:00.000 --> 00:01.000\r\nx\r\n";
        let s = parse(doc).unwrap();
        assert_eq!(s.cues[0].text, "x");
    }

    #[test]
    fn normalizes_lone_carriage_returns() {
        let doc = "WEBVTT\r\r00:00.000 --> 00:01.000\rx";
        let s = parse(doc).unwrap();
        assert_eq!(s.cues[0].text, "x");
    }

    #[test]
    fn sorts_out_of_order_cues() {
        let doc = "WEBVTT\n\n00:05.000 --> 00:06.000\nlate\n\n00:01.000 --> 00:02.000\nearly";
        let s = parse(doc).unwrap();
        assert_eq!(s.cues[0].text, "early");
        assert_eq!(s.cues[1].text, "late");
    }

    #[test]
    fn rejects_end_before_start() {
        assert!(parse("WEBVTT\n\n00:05.000 --> 00:02.000\nx").is_err());
    }

    #[test]
    fn parses_full_hh_mm_ss_mmm() {
        assert_eq!(parse_timestamp("01:02:03.456").unwrap(), 3_723_456);
    }

    #[test]
    fn parses_mm_ss_mmm() {
        assert_eq!(parse_timestamp("00:05.000").unwrap(), 5_000);
    }

    #[test]
    fn rejects_missing_millis() {
        assert!(parse_timestamp("00:05").is_err());
    }

    #[test]
    fn rejects_non_three_digit_millis() {
        assert!(parse_timestamp("00:05.5").is_err());
    }

    #[test]
    fn rejects_out_of_range_seconds() {
        assert!(parse_timestamp("00:75.000").is_err());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_timestamp("abc").is_err());
    }
}
