use super::Codec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WvttCodec;

impl Codec for WvttCodec {
    fn rfc6381(&self) -> String {
        "wvtt".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{Codec, WvttCodec};

    #[test]
    fn rfc6381_identifies_webvtt() {
        assert_eq!(WvttCodec.rfc6381(), "wvtt");
    }
}
