use std::{fmt, str::FromStr};

use mp4_atom::Mp4a;

use super::CodecConfigError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AacCodec {
    profile: u8,
}

impl AacCodec {
    pub fn from_atom(atom: &Mp4a) -> Self {
        Self {
            profile: atom.esds.es_desc.dec_config.dec_specific.profile,
        }
    }
}

impl fmt::Display for AacCodec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "mp4a.40.{}", self.profile)
    }
}

impl FromStr for AacCodec {
    type Err = CodecConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some(profile) = value.strip_prefix("mp4a.40.") else {
            return Err(CodecConfigError::InvalidRfc6381(value.to_owned()));
        };
        if profile.is_empty() || !profile.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(CodecConfigError::InvalidRfc6381(value.to_owned()));
        }

        let profile = profile
            .parse()
            .map_err(|_| CodecConfigError::InvalidRfc6381(value.to_owned()))?;

        Ok(Self { profile })
    }
}
