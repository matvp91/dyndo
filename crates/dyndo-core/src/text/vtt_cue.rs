//! The parsed WebVTT cue type and timestamp parsing.

use super::error::CoreTextError;

/// A single parsed WebVTT cue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VttCue {
    /// Optional cue identifier line (the line before the timestamp, if any).
    pub id: Option<String>,
    /// Start time, in milliseconds.
    pub start_ms: u64,
    /// End time, in milliseconds.
    pub end_ms: u64,
    /// Cue settings: the text after the end timestamp on the `-->` line,
    /// verbatim; `None` if empty.
    pub settings: Option<String>,
    /// Cue payload text, verbatim (may be multi-line, joined with `\n`).
    pub payload: String,
}

/// Parse a WebVTT timestamp (`HH:MM:SS.mmm` or `MM:SS.mmm`) into milliseconds.
pub(crate) fn parse_timestamp(s: &str) -> Result<u64, CoreTextError> {
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
