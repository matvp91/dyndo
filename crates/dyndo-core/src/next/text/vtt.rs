use super::{Cue, TimedText};

pub(super) fn parse(text: &str) -> Result<TimedText, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines().enumerate().peekable();
    let Some((_, header)) = lines.next() else {
        return Err("the WEBVTT header is missing".to_string());
    };
    if header != "WEBVTT" && !header.starts_with("WEBVTT ") && !header.starts_with("WEBVTT\t") {
        return Err("the WEBVTT header is missing".to_string());
    }

    if lines.peek().is_some_and(|(_, line)| !line.is_empty()) {
        return Err("the WEBVTT header must be followed by a blank line".to_string());
    }
    lines.next_if(|(_, line)| line.is_empty());

    let mut cues = Vec::new();
    while let Some((line_number, first_line)) = next_nonempty(&mut lines) {
        let (timing_line_number, timing) = if first_line.contains("-->") {
            (line_number, first_line)
        } else {
            let Some((index, timing)) = lines.next() else {
                return Err(format!(
                    "cue identifier on line {line_number} has no timing line"
                ));
            };
            (index + 1, timing)
        };

        let (start_ms, end_ms) = parse_timing(timing, timing_line_number)?;
        let mut payload = Vec::new();
        while let Some((_, line)) = lines.next_if(|(_, line)| !line.is_empty()) {
            payload.push(line);
        }
        lines.next_if(|(_, line)| line.is_empty());

        if payload.is_empty() {
            return Err(format!(
                "cue on line {timing_line_number} has no text payload"
            ));
        }

        cues.push(Cue {
            start_ms,
            end_ms,
            text: payload.join("\n"),
        });
    }

    Ok(TimedText { cues })
}

fn next_nonempty<'a, I>(lines: &mut std::iter::Peekable<I>) -> Option<(usize, &'a str)>
where
    I: Iterator<Item = (usize, &'a str)>,
{
    while lines.peek().is_some_and(|(_, line)| line.is_empty()) {
        lines.next();
    }
    lines.next().map(|(index, line)| (index + 1, line))
}

fn parse_timing(line: &str, line_number: usize) -> Result<(u64, u64), String> {
    let Some((start, end)) = line.split_once("-->") else {
        return Err(format!("cue on line {line_number} has no timing separator"));
    };
    let start = start.trim();
    let end = end.trim();
    if end.split_whitespace().count() != 1 {
        return Err(format!(
            "cue settings on line {line_number} are not supported"
        ));
    }

    let start_ms = parse_timestamp(start, line_number)?;
    let end_ms = parse_timestamp(end, line_number)?;
    if end_ms < start_ms {
        return Err(format!("cue on line {line_number} ends before it starts"));
    }
    Ok((start_ms, end_ms))
}

fn parse_timestamp(timestamp: &str, line_number: usize) -> Result<u64, String> {
    let invalid = || format!("invalid timestamp `{timestamp}` on line {line_number}");
    let parts: Vec<_> = timestamp.split(':').collect();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [minutes, seconds] if minutes.len() == 2 => (0, *minutes, *seconds),
        [hours, minutes, seconds] if hours.len() >= 2 && minutes.len() == 2 => (
            hours.parse::<u64>().map_err(|_| invalid())?,
            *minutes,
            *seconds,
        ),
        _ => return Err(invalid()),
    };

    let Some((seconds, milliseconds)) = seconds.split_once('.') else {
        return Err(invalid());
    };
    if seconds.len() != 2 || milliseconds.len() != 3 {
        return Err(invalid());
    }

    let minutes = minutes.parse::<u64>().map_err(|_| invalid())?;
    let seconds = seconds.parse::<u64>().map_err(|_| invalid())?;
    let milliseconds = milliseconds.parse::<u64>().map_err(|_| invalid())?;
    if minutes > 59 || seconds > 59 {
        return Err(invalid());
    }

    hours
        .checked_mul(60)
        .and_then(|value| value.checked_add(minutes))
        .and_then(|value| value.checked_mul(60))
        .and_then(|value| value.checked_add(seconds))
        .and_then(|value| value.checked_mul(1_000))
        .and_then(|value| value.checked_add(milliseconds))
        .ok_or_else(invalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_returns_cues_with_both_timestamp_forms_and_multiline_text() {
        let input = "WEBVTT\n\n00:01.250 --> 00:03.000\nFirst line\nsecond line\n\nidentifier\n01:02:03.004 --> 01:02:05.006\nAnother cue\n";
        let expected = TimedText {
            cues: vec![
                Cue {
                    start_ms: 1_250,
                    end_ms: 3_000,
                    text: "First line\nsecond line".to_string(),
                },
                Cue {
                    start_ms: 3_723_004,
                    end_ms: 3_725_006,
                    text: "Another cue".to_string(),
                },
            ],
        };

        assert_eq!(parse(input), Ok(expected));
    }

    #[test]
    fn parse_accepts_a_byte_order_mark_and_crlf_line_endings() {
        let input = "\u{feff}WEBVTT\r\n\r\n00:00.000 --> 00:01.000\r\nText\r\n";

        assert!(parse(input).is_ok());
    }

    #[test]
    fn parse_rejects_a_missing_header() {
        let error = parse("00:00.000 --> 00:01.000\nText\n").unwrap_err();

        assert_eq!(error, "the WEBVTT header is missing");
    }

    #[test]
    fn parse_rejects_cue_settings() {
        let input = "WEBVTT\n\n00:00.000 --> 00:01.000 line:10%\nText\n";
        let error = parse(input).unwrap_err();

        assert_eq!(error, "cue settings on line 3 are not supported");
    }

    #[test]
    fn parse_rejects_a_cue_that_ends_before_it_starts() {
        let input = "WEBVTT\n\n00:02.000 --> 00:01.000\nText\n";
        let error = parse(input).unwrap_err();

        assert_eq!(error, "cue on line 3 ends before it starts");
    }
}
