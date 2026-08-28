use super::Codec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ac3Codec;

impl Codec for Ac3Codec {
    fn rfc6381(&self) -> String {
        "ac-3".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{Ac3Codec, Codec};

    #[test]
    fn rfc6381_identifies_ac3() {
        assert_eq!(Ac3Codec.rfc6381(), "ac-3");
    }
}
