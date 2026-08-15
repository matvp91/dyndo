use std::{fmt, str::FromStr};

use super::CodecConfigError;

/// A WebVTT-in-ISOBMFF codec configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WvttCodec;

impl fmt::Display for WvttCodec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("wvtt")
    }
}

impl FromStr for WvttCodec {
    type Err = CodecConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "wvtt" {
            Ok(Self)
        } else {
            Err(CodecConfigError::InvalidRfc6381(value.to_owned()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WvttCodec;

    #[test]
    fn wvtt_codec_formats_as_its_rfc_6381_identifier() {
        assert_eq!(WvttCodec.to_string(), "wvtt");
    }
}
