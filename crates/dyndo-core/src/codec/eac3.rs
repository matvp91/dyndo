use super::Codec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Eac3Codec;

impl Codec for Eac3Codec {
    fn rfc6381(&self) -> String {
        "ec-3".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{Codec, Eac3Codec};

    #[test]
    fn rfc6381_identifies_eac3() {
        assert_eq!(Eac3Codec.rfc6381(), "ec-3");
    }
}
