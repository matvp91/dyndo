use std::fmt::Debug;

mod aac;
mod av1;
mod avc;
mod hevc;

pub use aac::AacCodec;
pub use av1::Av1Codec;
pub use avc::AvcCodec;
pub use hevc::HevcCodec;

pub trait Codec: Debug {
    fn rfc6381(&self) -> String;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecConfig {
    Avc(AvcCodec),
    Av1(Av1Codec),
    Hevc(HevcCodec),
    Aac(AacCodec),
}

impl CodecConfig {
    pub fn rfc6381(&self) -> String {
        Codec::rfc6381(self)
    }
}

impl Codec for CodecConfig {
    fn rfc6381(&self) -> String {
        match self {
            Self::Avc(codec) => codec.rfc6381(),
            Self::Av1(codec) => codec.rfc6381(),
            Self::Hevc(codec) => codec.rfc6381(),
            Self::Aac(codec) => codec.rfc6381(),
        }
    }
}
