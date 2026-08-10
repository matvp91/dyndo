use mp4_atom::{Hev1, Hvc1, Hvcc};

use super::Codec;

pub trait HevcEntry {
    fn prefix(&self) -> &'static str;
    fn configuration(&self) -> &Hvcc;
}

impl HevcEntry for Hvc1 {
    fn prefix(&self) -> &'static str {
        "hvc1"
    }

    fn configuration(&self) -> &Hvcc {
        &self.hvcc
    }
}

impl HevcEntry for Hev1 {
    fn prefix(&self) -> &'static str {
        "hev1"
    }

    fn configuration(&self) -> &Hvcc {
        &self.hvcc
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HevcCodec {
    rfc6381: String,
}

impl HevcCodec {
    pub fn new(codec: &impl HevcEntry) -> Self {
        Self {
            rfc6381: rfc6381(codec.prefix(), codec.configuration()),
        }
    }
}

impl Codec for HevcCodec {
    fn rfc6381(&self) -> String {
        self.rfc6381.clone()
    }
}

fn rfc6381(prefix: &str, configuration: &Hvcc) -> String {
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
