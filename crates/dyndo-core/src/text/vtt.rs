//! WebVTT document parsing: `&str` → [`WebVtt`].

use super::error::CoreTextError;
use super::vtt_cue::{parse_timestamp, VttCue};

/// A parsed WebVTT document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebVtt {
    /// The document preamble — the `WEBVTT` signature line, any header text, and
    /// `STYLE`/`REGION` blocks (everything before the first cue), verbatim.
    /// Becomes the `vttC` configuration payload.
    pub config: String,
    /// The cues, sorted by `(start_ms, end_ms)`.
    pub cues: Vec<VttCue>,
}

/// Parse a WebVTT document into a [`WebVtt`].
///
/// # Errors
/// Returns [`CoreTextError::Vtt`] if the `WEBVTT` signature is missing, a
/// timestamp is malformed, or a cue's end precedes its start.
pub fn parse(input: &str) -> Result<WebVtt, CoreTextError> {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");

    if !normalized.lines().next().unwrap_or("").starts_with("WEBVTT") {
        return Err(CoreTextError::Vtt("missing WEBVTT signature".into()));
    }

    let blocks = split_blocks(&normalized);
    let first_cue = blocks.iter().position(|b| is_cue(b));

    let (config, cue_blocks): (String, &[String]) = match first_cue {
        Some(i) => (blocks[..i].join("\n\n"), &blocks[i..]),
        None => (blocks.join("\n\n"), &[]),
    };

    let mut cues = Vec::new();
    for block in cue_blocks {
        if is_cue(block) {
            cues.push(parse_cue(block)?);
        }
        // Non-cue blocks between cues (e.g. NOTE comments) are skipped.
    }
    cues.sort_by_key(|c| (c.start_ms, c.end_ms));

    Ok(WebVtt { config, cues })
}

/// Split into blocks separated by one or more blank lines.
fn split_blocks(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for line in s.lines() {
        if line.trim().is_empty() {
            if !cur.is_empty() {
                out.push(cur.join("\n"));
                cur.clear();
            }
        } else {
            cur.push(line);
        }
    }
    if !cur.is_empty() {
        out.push(cur.join("\n"));
    }
    out
}

/// A block is a cue if it is not a `NOTE` comment and carries a `-->` line.
fn is_cue(block: &str) -> bool {
    !block.starts_with("NOTE") && block.lines().any(|l| l.contains("-->"))
}

fn parse_cue(block: &str) -> Result<VttCue, CoreTextError> {
    let mut lines = block.lines();
    let first = lines.next().unwrap_or("");
    let (id, timing) = if first.contains("-->") {
        (None, first)
    } else {
        let timing = lines
            .next()
            .ok_or_else(|| CoreTextError::Vtt("cue id without a timing line".into()))?;
        (Some(first.to_string()), timing)
    };

    let (start_raw, rest) = timing
        .split_once("-->")
        .ok_or_else(|| CoreTextError::Vtt("timing line has no '-->'".into()))?;
    let start_ms = parse_timestamp(start_raw.trim())?;

    let rest = rest.trim_start();
    let mut it = rest.splitn(2, char::is_whitespace);
    let end_ms = parse_timestamp(it.next().unwrap_or("").trim())?;
    let settings = it
        .next()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if end_ms < start_ms {
        return Err(CoreTextError::Vtt(format!(
            "cue end {end_ms} precedes start {start_ms}"
        )));
    }

    let payload = lines.collect::<Vec<_>>().join("\n");
    Ok(VttCue {
        id,
        start_ms,
        end_ms,
        settings,
        payload,
    })
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
        let v = parse("WEBVTT\n\n00:00.000 --> 00:02.000\nHello").unwrap();
        assert_eq!(v.config, "WEBVTT");
        assert_eq!(v.cues.len(), 1);
        assert_eq!(v.cues[0].id, None);
        assert_eq!(v.cues[0].start_ms, 0);
        assert_eq!(v.cues[0].end_ms, 2000);
        assert_eq!(v.cues[0].settings, None);
        assert_eq!(v.cues[0].payload, "Hello");
    }

    #[test]
    fn captures_cue_id_and_settings() {
        let v = parse("WEBVTT\n\nintro\n00:00.000 --> 00:02.000 align:start line:90%\nHi").unwrap();
        assert_eq!(v.cues[0].id.as_deref(), Some("intro"));
        assert_eq!(v.cues[0].settings.as_deref(), Some("align:start line:90%"));
        assert_eq!(v.cues[0].payload, "Hi");
    }

    #[test]
    fn joins_multiline_payload() {
        let v = parse("WEBVTT\n\n00:00.000 --> 00:02.000\nline one\nline two").unwrap();
        assert_eq!(v.cues[0].payload, "line one\nline two");
    }

    #[test]
    fn captures_style_block_into_config() {
        let doc = "WEBVTT\n\nSTYLE\n::cue { color: yellow }\n\n00:00.000 --> 00:01.000\nx";
        let v = parse(doc).unwrap();
        assert_eq!(v.config, "WEBVTT\n\nSTYLE\n::cue { color: yellow }");
        assert_eq!(v.cues.len(), 1);
    }

    #[test]
    fn skips_note_blocks_between_cues() {
        let doc = "WEBVTT\n\n00:00.000 --> 00:01.000\na\n\nNOTE this is a comment\n\n00:02.000 --> 00:03.000\nb";
        let v = parse(doc).unwrap();
        assert_eq!(v.cues.len(), 2);
        assert_eq!(v.cues[0].payload, "a");
        assert_eq!(v.cues[1].payload, "b");
    }

    #[test]
    fn normalizes_crlf_and_bom() {
        let doc = "\u{feff}WEBVTT\r\n\r\n00:00.000 --> 00:01.000\r\nx\r\n";
        let v = parse(doc).unwrap();
        assert_eq!(v.config, "WEBVTT");
        assert_eq!(v.cues[0].payload, "x");
    }

    #[test]
    fn sorts_out_of_order_cues() {
        let doc = "WEBVTT\n\n00:05.000 --> 00:06.000\nlate\n\n00:01.000 --> 00:02.000\nearly";
        let v = parse(doc).unwrap();
        assert_eq!(v.cues[0].payload, "early");
        assert_eq!(v.cues[1].payload, "late");
    }

    #[test]
    fn rejects_end_before_start() {
        assert!(parse("WEBVTT\n\n00:05.000 --> 00:02.000\nx").is_err());
    }
}
