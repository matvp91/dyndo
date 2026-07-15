//! The text module's error type, [`CoreTextError`].

/// Anything that can go wrong parsing WebVTT or packing it into a `wvtt` track.
#[derive(Debug, thiserror::Error)]
pub enum CoreTextError {
    /// A WebVTT document was malformed or could not be parsed.
    #[error("invalid WebVTT: {0}")]
    Vtt(String),
    /// The `wvtt` track could not be encoded.
    #[error("failed to encode wvtt: {0}")]
    Wvtt(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vtt_error_displays_message() {
        let e = CoreTextError::Vtt("bad timestamp".into());
        assert_eq!(e.to_string(), "invalid WebVTT: bad timestamp");
    }

    #[test]
    fn wvtt_error_displays_message() {
        let e = CoreTextError::Wvtt("short buffer".into());
        assert_eq!(e.to_string(), "failed to encode wvtt: short buffer");
    }
}
