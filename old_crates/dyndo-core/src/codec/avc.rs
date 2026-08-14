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

#[cfg(test)]
mod tests {
    use mp4_atom::{Avc1, Avcc};

    use super::{AvcCodec, Codec};

    #[test]
    fn rfc6381_uses_lowercase_hex_profile_components() {
        let codec = AvcCodec {
            profile: 0x42,
            compatibility: 0xc0,
            level: 0x0a,
            nal_length_size: 4,
            sequence_parameter_sets: Vec::new(),
            picture_parameter_sets: Vec::new(),
        };

        assert_eq!(codec.rfc6381(), "avc1.42c00a");
    }

    #[test]
    fn new_preserves_h264_configuration() {
        let entry = Avc1 {
            avcc: Avcc {
                avc_profile_indication: 0x64,
                profile_compatibility: 0,
                avc_level_indication: 0x1f,
                length_size: 2,
                sequence_parameter_sets: vec![vec![1, 2]],
                picture_parameter_sets: vec![vec![3]],
                ..Avcc::default()
            },
            ..Avc1::default()
        };
        let codec = AvcCodec::new(&entry);

        assert_eq!(codec.rfc6381(), "avc1.64001f");
        assert_eq!(codec.nal_length_size(), 2);
        assert_eq!(codec.sequence_parameter_sets(), [vec![1, 2]]);
        assert_eq!(codec.picture_parameter_sets(), [vec![3]]);
    }
}
