use std::{fmt, str::FromStr};

use mp4_atom::Avc1;

use super::CodecConfigError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvcCodec {
    profile: u8,
    compatibility: u8,
    level: u8,
}

impl AvcCodec {
    pub fn from_atom(atom: &Avc1) -> Self {
        Self {
            profile: atom.avcc.avc_profile_indication,
            compatibility: atom.avcc.profile_compatibility,
            level: atom.avcc.avc_level_indication,
        }
    }
}

impl fmt::Display for AvcCodec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "avc1.{:02x}{:02x}{:02x}",
            self.profile, self.compatibility, self.level
        )
    }
}

impl FromStr for AvcCodec {
    type Err = CodecConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some(components) = value.strip_prefix("avc1.") else {
            return Err(CodecConfigError::InvalidRfc6381(value.to_owned()));
        };
        if components.len() != 6 {
            return Err(CodecConfigError::InvalidRfc6381(value.to_owned()));
        }

        let components = u32::from_str_radix(components, 16)
            .map_err(|_| CodecConfigError::InvalidRfc6381(value.to_owned()))?;

        Ok(Self {
            profile: (components >> 16) as u8,
            compatibility: (components >> 8) as u8,
            level: components as u8,
        })
    }
}
