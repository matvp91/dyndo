use std::{fmt, str::FromStr};

use super::CodecConfigError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ac3Codec;

impl fmt::Display for Ac3Codec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ac-3")
    }
}

impl FromStr for Ac3Codec {
    type Err = CodecConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "ac-3" {
            Ok(Self)
        } else {
            Err(CodecConfigError::InvalidRfc6381(value.to_owned()))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Eac3Codec;

impl fmt::Display for Eac3Codec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ec-3")
    }
}

impl FromStr for Eac3Codec {
    type Err = CodecConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "ec-3" {
            Ok(Self)
        } else {
            Err(CodecConfigError::InvalidRfc6381(value.to_owned()))
        }
    }
}
