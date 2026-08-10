use std::fmt::Debug;

mod aac;
mod ac3;
mod av1;
mod avc;
mod eac3;
mod hevc;
mod wvtt;

pub use aac::AacCodec;
pub use ac3::Ac3Codec;
pub use av1::Av1Codec;
pub use avc::AvcCodec;
pub use eac3::Eac3Codec;
pub use hevc::HevcCodec;
pub use wvtt::WvttCodec;

pub trait Codec: Debug {
    fn rfc6381(&self) -> String;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecConfig {
    Avc(AvcCodec),
    Av1(Av1Codec),
    Hevc(HevcCodec),
    Aac(AacCodec),
    Ac3(Ac3Codec),
    Eac3(Eac3Codec),
    Wvtt(WvttCodec),
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
            Self::Ac3(codec) => codec.rfc6381(),
            Self::Eac3(codec) => codec.rfc6381(),
            Self::Wvtt(codec) => codec.rfc6381(),
        }
    }
}
