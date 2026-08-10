use super::Codec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ac3Codec;

impl Codec for Ac3Codec {
    fn rfc6381(&self) -> String {
        "ac-3".to_string()
    }
}
