use crate::codec::CodecConfig;
use crate::drm::{EncryptionConfig, EncryptionScheme};

mod avc;
mod fragment;
mod init;
mod mp4;
mod sample;

#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error("invalid or unsupported CMAF initialization segment")]
    InvalidInit,
    #[error("unsupported encryption scheme")]
    UnsupportedScheme,
    #[error("unsupported codec for CMAF encryption")]
    UnsupportedCodec,
    #[error("initialization segment is too large")]
    TooLarge,
    #[error("invalid or unsupported CMAF media segment")]
    InvalidMedia,
    #[error("could not generate a sample IV")]
    Random,
    #[error("invalid MP4 box")]
    Atom(#[from] mp4_atom::Error),
}

#[derive(Clone)]
pub(super) struct Encryptor {
    config: EncryptionConfig,
}

impl Encryptor {
    pub(super) fn new(config: EncryptionConfig, codec: &CodecConfig) -> Result<Self, Error> {
        if config.protection.scheme != EncryptionScheme::Cenc {
            return Err(Error::UnsupportedScheme);
        }
        if !matches!(codec, CodecConfig::Avc(_) | CodecConfig::Aac(_)) {
            return Err(Error::UnsupportedCodec);
        }
        Ok(Self { config })
    }

    pub(super) fn config(&self) -> &EncryptionConfig {
        &self.config
    }

    pub(super) fn encrypt_init(&self, init: &[u8]) -> Result<Vec<u8>, Error> {
        init::InitSegmentEncryptor::new(&self.config).encrypt(init)
    }

    pub(super) fn encrypt_media(
        &self,
        media: &[u8],
        codec: &CodecConfig,
    ) -> Result<Vec<u8>, Error> {
        fragment::FragmentEncryptor::new(&self.config, codec)?.encrypt(media)
    }
}
