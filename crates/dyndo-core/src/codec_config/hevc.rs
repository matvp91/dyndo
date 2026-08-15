use std::{fmt, str::FromStr};

use mp4_atom::{Hev1, Hvc1, Hvcc};

use super::CodecConfigError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HevcCodec {
    rfc6381: String,
}

impl HevcCodec {
    pub fn from_hev1(atom: &Hev1) -> Self {
        Self {
            rfc6381: from_atom("hev1", &atom.hvcc),
        }
    }

    pub fn from_hvc1(atom: &Hvc1) -> Self {
        Self {
            rfc6381: from_atom("hvc1", &atom.hvcc),
        }
    }
}

impl fmt::Display for HevcCodec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.rfc6381)
    }
}

impl FromStr for HevcCodec {
    type Err = CodecConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.starts_with("hev1.") || value.starts_with("hvc1.") {
            Ok(Self {
                rfc6381: value.to_owned(),
            })
        } else {
            Err(CodecConfigError::InvalidRfc6381(value.to_owned()))
        }
    }
}

fn from_atom(prefix: &str, configuration: &Hvcc) -> String {
    let profile_space = match configuration.general_profile_space {
        0 => String::new(),
        value => ((b'A' + value - 1) as char).to_string(),
    };
    let compatibility =
        u32::from_be_bytes(configuration.general_profile_compatibility_flags).reverse_bits();
    let tier = if configuration.general_tier_flag {
        'H'
    } else {
        'L'
    };
    let mut codec = format!(
        "{prefix}.{profile_space}{}.{compatibility:x}.{tier}{}",
        configuration.general_profile_idc, configuration.general_level_idc
    );

    if let Some(end) = configuration
        .general_constraint_indicator_flags
        .iter()
        .rposition(|&byte| byte != 0)
    {
        for byte in &configuration.general_constraint_indicator_flags[..=end] {
            codec.push_str(&format!(".{byte:02x}"));
        }
    }

    codec
}
