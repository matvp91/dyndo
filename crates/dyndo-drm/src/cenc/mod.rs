use crate::encryption_config::{EncryptionConfig, EncryptionScheme};

mod fragment;
mod h264;
mod init;
mod mp4;
mod sample;

pub use sample::SampleEncryption;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid or unsupported CMAF initialization segment")]
    InvalidInit,
    #[error("unsupported encryption scheme")]
    UnsupportedScheme,
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
pub struct Encryptor {
    config: EncryptionConfig,
    sample_encryption: SampleEncryption,
}

impl Encryptor {
    pub fn new(
        config: EncryptionConfig,
        sample_encryption: SampleEncryption,
    ) -> Result<Self, Error> {
        if config.scheme != EncryptionScheme::Cenc {
            return Err(Error::UnsupportedScheme);
        }
        Ok(Self {
            config,
            sample_encryption,
        })
    }

    pub fn config(&self) -> &EncryptionConfig {
        &self.config
    }

    pub fn encrypt_init(&self, init: &[u8]) -> Result<Vec<u8>, Error> {
        init::InitSegmentEncryptor::new(&self.config).encrypt(init)
    }

    pub fn encrypt_media(&self, media: &[u8]) -> Result<Vec<u8>, Error> {
        fragment::FragmentEncryptor::new(&self.config, &self.sample_encryption)?.encrypt(media)
    }
}
