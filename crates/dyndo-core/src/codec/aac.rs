use mp4_atom::Mp4a;

use super::Codec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AacCodec {
    profile: u8,
}

impl AacCodec {
    pub fn new(codec: &Mp4a) -> Self {
        Self {
            profile: codec.esds.es_desc.dec_config.dec_specific.profile,
        }
    }
}

impl Codec for AacCodec {
    fn rfc6381(&self) -> String {
        format!("mp4a.40.{}", self.profile)
    }
}

#[cfg(test)]
mod tests {
    use super::{AacCodec, Codec};

    #[test]
    fn rfc6381_includes_the_audio_object_type() {
        let codec = AacCodec { profile: 2 };

        assert_eq!(codec.rfc6381(), "mp4a.40.2");
    }
}
