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
    use mp4_atom::{Any, Codec as Mp4Codec, DecodeMaybe};

    use super::{AacCodec, Codec};

    #[test]
    fn rfc6381_includes_the_audio_object_type() {
        let codec = AacCodec { profile: 2 };

        assert_eq!(codec.rfc6381(), "mp4a.40.2");
    }

    #[test]
    fn new_reads_the_audio_object_type_from_esds() {
        let mut input =
            include_bytes!("../../tests/fixtures/one-second-silence-aac.mp4").as_slice();

        while let Some(atom) = Any::decode_maybe(&mut input).unwrap() {
            let Any::Moov(moov) = atom else {
                continue;
            };
            let Mp4Codec::Mp4a(entry) = &moov.trak[0].mdia.minf.stbl.stsd.codecs[0] else {
                panic!("fixture must contain an AAC sample entry");
            };

            assert_eq!(AacCodec::new(entry).rfc6381(), "mp4a.40.2");
            return;
        }

        panic!("fixture must contain a movie box");
    }
}
