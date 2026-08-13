use std::ops::Range;

use bytes::Bytes;
use dyndo_drm::cenc::{Encryptor, SampleEncryption};
use dyndo_drm::cpix_parser::Cpix;
use dyndo_drm::encryption_config::{EncryptionConfig, TrackMetadata};
use opendal::Operator;

use super::{CmafMetadata, CmafReadError, ResolvedCmafTrack};

#[derive(Clone)]
pub struct EncryptedCmafTrack {
    track: ResolvedCmafTrack,
    encryptor: Encryptor,
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
        let sample_encryption = match track.codec() {
            crate::codec::CodecConfig::Avc(codec) => SampleEncryption::Avc {
                nal_length_size: codec.nal_length_size(),
                sequence_parameter_sets: codec.sequence_parameter_sets().to_vec(),
                picture_parameter_sets: codec.picture_parameter_sets().to_vec(),
            },
            crate::codec::CodecConfig::Aac(_) => SampleEncryption::FullSample,
            _ => return Err(CmafEncryptionError::UnsupportedTrack),
        };
        let encryptor = Encryptor::new(config, sample_encryption)?;

        Ok(Self { track, encryptor })
    }

    pub fn track(&self) -> &ResolvedCmafTrack {
        &self.track
    }

    pub fn config(&self) -> &EncryptionConfig {
        self.encryptor.config()
    }

    pub async fn initialization(&self, operator: &Operator) -> Result<Bytes, CmafEncryptionError> {
        let init = self
            .track
            .read_range(operator, self.track.init_segment().byte_range())
            .await?;
        Ok(self.encryptor.encrypt_init(&init)?.into())
    }

    pub async fn media(
        &self,
        operator: &Operator,
        range: Range<u64>,
    ) -> Result<Bytes, CmafEncryptionError> {
        let media = self.track.read_range(operator, range).await?;
        Ok(self.encryptor.encrypt_media(&media)?.into())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CmafEncryptionError {
    #[error(transparent)]
    Config(#[from] dyndo_drm::encryption_config::Error),
    #[error(transparent)]
    Read(#[from] CmafReadError),
    #[error(transparent)]
    Encrypt(#[from] dyndo_drm::cenc::Error),
    #[error("this track is not supported by the current encryption implementation")]
    UnsupportedTrack,
}
