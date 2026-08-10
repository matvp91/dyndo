use super::Codec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WvttCodec;

impl Codec for WvttCodec {
    fn rfc6381(&self) -> String {
        "wvtt".to_string()
    }
}
