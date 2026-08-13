use bytes::Bytes;
use opendal::Operator;

use super::encryption::Encryptor;
use super::{CmafMetadata, CmafReadError, ResolvedCmafTrack, ServedSegment};
use crate::drm::{Cpix, Protection, TrackMetadata};

impl ResolvedCmafTrack {
    pub fn with_protection(mut self, cpix: &Cpix) -> Result<Self, CmafEncryptionError> {
        self.protect(cpix)?;
        Ok(self)
    }

    pub(crate) fn protect(&mut self, cpix: &Cpix) -> Result<(), CmafEncryptionError> {
        let metadata = match self.metadata() {
            CmafMetadata::Audio(_) => Some(TrackMetadata::Audio),
            CmafMetadata::Video(video) => TrackMetadata::Video {
                width: video.width,
                height: video.height,
            }
            .into(),
            CmafMetadata::Text(_) => None,
        };
        let Some(metadata) = metadata else {
            return Ok(());
        };
        let config = cpix.encryption_config_for(metadata)?;
        let encryptor = Encryptor::new(config, self.codec())?;
        self.encryptor = Some(encryptor);
        Ok(())
    }

    pub fn protection(&self) -> Option<&Protection> {
        self.encryptor
            .as_ref()
            .map(Encryptor::config)
            .map(|config| &config.protection)
    }

    pub async fn read_initialization(
        &self,
        operator: &Operator,
    ) -> Result<Bytes, CmafEncryptionError> {
        let init = self
            .read_range(operator, self.init_segment().byte_range())
            .await?;
        match &self.encryptor {
            Some(encryptor) => Ok(encryptor.encrypt_init(&init)?.into()),
            None => Ok(init),
        }
    }

    pub async fn read_media(
        &self,
        operator: &Operator,
        segment: &ServedSegment<'_>,
    ) -> Result<Bytes, CmafEncryptionError> {
        let media = self.read_range(operator, segment.byte_range()).await?;
        match &self.encryptor {
            Some(encryptor) => Ok(encryptor.encrypt_media(&media, self.codec())?.into()),
            None => Ok(media),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CmafEncryptionError {
    #[error(transparent)]
    Config(#[from] crate::drm::Error),
    #[error(transparent)]
    Read(#[from] CmafReadError),
    #[error("CMAF encryption failed: {0}")]
    Encrypt(String),
}

impl From<super::encryption::Error> for CmafEncryptionError {
    fn from(error: super::encryption::Error) -> Self {
        Self::Encrypt(error.to_string())
    }
}
