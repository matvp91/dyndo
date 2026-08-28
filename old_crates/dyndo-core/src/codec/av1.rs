use mp4_atom::Av01;

use super::Codec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Av1Codec {
    profile: u8,
    level: u8,
    tier: char,
    bit_depth: u8,
}

impl Av1Codec {
    pub fn new(codec: &Av01) -> Self {
        Self {
            profile: codec.av1c.seq_profile,
            level: codec.av1c.seq_level_idx_0,
            tier: if codec.av1c.seq_tier_0 { 'H' } else { 'M' },
            bit_depth: if codec.av1c.twelve_bit {
                12
            } else if codec.av1c.high_bitdepth {
                10
            } else {
                8
            },
        }
    }
}

impl Codec for Av1Codec {
    fn rfc6381(&self) -> String {
        format!(
            "av01.{}.{:02}{}.{:02}",
            self.profile, self.level, self.tier, self.bit_depth
        )
    }
}

#[cfg(test)]
mod tests {
    use mp4_atom::{Av01, Av1c};

    use super::{Av1Codec, Codec};

    #[test]
    fn rfc6381_includes_profile_level_tier_and_bit_depth() {
        let codec = Av1Codec {
            profile: 0,
            level: 8,
            tier: 'M',
            bit_depth: 10,
        };

        assert_eq!(codec.rfc6381(), "av01.0.08M.10");
    }

    #[test]
    fn new_maps_the_configuration_tier_and_bit_depth() {
        let entry = Av01 {
            av1c: Av1c {
                seq_profile: 2,
                seq_level_idx_0: 12,
                seq_tier_0: true,
                twelve_bit: true,
                ..Av1c::default()
            },
            ..Av01::default()
        };

        assert_eq!(Av1Codec::new(&entry).rfc6381(), "av01.2.12H.12");
    }
}
