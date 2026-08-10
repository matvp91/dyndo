use mp4_atom::Avc1;

use super::Codec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvcCodec {
    profile: u8,
    compatibility: u8,
    level: u8,
    nal_length_size: u8,
    sequence_parameter_sets: Vec<Vec<u8>>,
    picture_parameter_sets: Vec<Vec<u8>>,
}

impl AvcCodec {
    pub fn new(codec: &Avc1) -> Self {
        Self {
            profile: codec.avcc.avc_profile_indication,
            compatibility: codec.avcc.profile_compatibility,
            level: codec.avcc.avc_level_indication,
            nal_length_size: codec.avcc.length_size,
            sequence_parameter_sets: codec.avcc.sequence_parameter_sets.clone(),
            picture_parameter_sets: codec.avcc.picture_parameter_sets.clone(),
        }
    }

    pub fn nal_length_size(&self) -> u8 {
        self.nal_length_size
    }

    pub fn sequence_parameter_sets(&self) -> &[Vec<u8>] {
        &self.sequence_parameter_sets
    }

    pub fn picture_parameter_sets(&self) -> &[Vec<u8>] {
        &self.picture_parameter_sets
    }
}

impl Codec for AvcCodec {
    fn rfc6381(&self) -> String {
        format!(
            "avc1.{:02x}{:02x}{:02x}",
            self.profile, self.compatibility, self.level
        )
    }
}
