use super::Codec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Eac3Codec;

impl Codec for Eac3Codec {
    fn rfc6381(&self) -> String {
        "ec-3".to_string()
    }
}
