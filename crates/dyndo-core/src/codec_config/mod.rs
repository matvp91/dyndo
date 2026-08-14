mod aac;
mod avc;
mod eac3;
mod hevc;

use std::{fmt, str::FromStr};

pub use aac::AacCodec;
pub use avc::AvcCodec;
pub use eac3::{Ac3Codec, Eac3Codec};
pub use hevc::HevcCodec;
use mp4_atom::Codec;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub enum CodecConfig {
    Avc(AvcCodec),
    Hevc(HevcCodec),
    Aac(AacCodec),
    Ac3(Ac3Codec),
    Eac3(Eac3Codec),
}

#[derive(Debug, Error)]
pub enum CodecConfigError {
    #[error("unsupported codec atom")]
    UnsupportedAtom,
    #[error("invalid RFC 6381 codec string: {0}")]
    InvalidRfc6381(String),
}

impl CodecConfig {
    pub fn from_atom(atom: &Codec) -> Result<Self, CodecConfigError> {
        match atom {
            Codec::Avc1(atom) => Ok(Self::Avc(AvcCodec::from_atom(atom))),
            Codec::Hvc1(atom) => Ok(Self::Hevc(HevcCodec::from_hvc1(atom))),
            Codec::Hev1(atom) => Ok(Self::Hevc(HevcCodec::from_hev1(atom))),
            Codec::Mp4a(atom) => Ok(Self::Aac(AacCodec::from_atom(atom))),
            Codec::Ac3(_) => Ok(Self::Ac3(Ac3Codec)),
            Codec::Eac3(_) => Ok(Self::Eac3(Eac3Codec)),
            _ => Err(CodecConfigError::UnsupportedAtom),
        }
    }
}

impl fmt::Display for CodecConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Avc(codec) => codec.fmt(formatter),
            Self::Hevc(codec) => codec.fmt(formatter),
            Self::Aac(codec) => codec.fmt(formatter),
            Self::Ac3(codec) => codec.fmt(formatter),
            Self::Eac3(codec) => codec.fmt(formatter),
        }
    }
}

impl FromStr for CodecConfig {
    type Err = CodecConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "ac-3" => Ok(Self::Ac3(Ac3Codec)),
            "ec-3" => Ok(Self::Eac3(Eac3Codec)),
            _ if value.starts_with("avc1.") => value.parse().map(Self::Avc),
            _ if value.starts_with("hvc1.") || value.starts_with("hev1.") => {
                value.parse().map(Self::Hevc)
            }
            _ if value.starts_with("mp4a.40.") => value.parse().map(Self::Aac),
            _ => Err(CodecConfigError::InvalidRfc6381(value.to_owned())),
        }
    }
}

impl From<CodecConfig> for String {
    fn from(config: CodecConfig) -> Self {
        config.to_string()
    }
}

impl TryFrom<String> for CodecConfig {
    type Error = CodecConfigError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
