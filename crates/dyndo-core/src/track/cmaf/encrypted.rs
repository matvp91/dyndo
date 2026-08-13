use bytes::Bytes;
use dyndo_crypt::cpix_parser::Cpix;
use dyndo_crypt::encryption_config::{EncryptionConfig, TrackMetadata};
use opendal::Operator;

use super::{CmafMetadata, CmafReadError, ResolvedCmafTrack};

#[derive(Clone)]
pub struct EncryptedCmafTrack {
    track: ResolvedCmafTrack,
    config: EncryptionConfig,
}

impl EncryptedCmafTrack {
    pub fn resolve(track: ResolvedCmafTrack, cpix: &Cpix) -> Result<Self, CmafEncryptionError> {
        let metadata = match track.metadata() {
            CmafMetadata::Audio(_) => TrackMetadata::Audio,
            CmafMetadata::Video(video) => TrackMetadata::Video {
                width: video.width,
                height: video.height,
            },
            CmafMetadata::Text(_) => return Err(CmafEncryptionError::UnsupportedTrack),
        };
        let config = cpix.encryption_config_for(metadata)?;

        Ok(Self { track, config })
    }

    pub fn track(&self) -> &ResolvedCmafTrack {
        &self.track
    }

    pub fn config(&self) -> &EncryptionConfig {
        &self.config
    }

    pub async fn initialization(&self, operator: &Operator) -> Result<Bytes, CmafEncryptionError> {
        let init = self
            .track
            .read_range(operator, self.track.init_segment().byte_range())
            .await?;
        Ok(dyndo_crypt::cenc::encrypt_init(&init, &self.config)?.into())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CmafEncryptionError {
    #[error(transparent)]
    Config(#[from] dyndo_crypt::encryption_config::Error),
    #[error(transparent)]
    Read(#[from] CmafReadError),
    #[error(transparent)]
    Encrypt(#[from] dyndo_crypt::cenc::Error),
    #[error("text tracks cannot be encrypted")]
    UnsupportedTrack,
}
