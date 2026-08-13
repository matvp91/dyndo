use dyndo_core::track::cmaf::CmafMetadata;
use uuid::Uuid;

use crate::cpix_parser::{ContentKeyUsageRule, Cpix};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Key(#[from] crate::cpix_parser::Error),
    #[error("no CPIX usage rule matches the track")]
    NoMatchingRule,
    #[error("multiple CPIX usage rules match the track")]
    AmbiguousRule,
    #[error("CPIX usage rule references missing content key {0}")]
    MissingKey(Uuid),
    #[error("unsupported common encryption scheme {0}")]
    UnsupportedScheme(String),
    #[error("text tracks cannot be encrypted")]
    UnsupportedTrack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionScheme {
    Cenc,
    Cbcs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncryptionConfig {
    pub scheme: EncryptionScheme,
    pub kid: Uuid,
    pub key: [u8; 16],
}

impl Cpix {
    pub fn encryption_config_for(
        &self,
        metadata: &CmafMetadata,
    ) -> Result<EncryptionConfig, Error> {
        if matches!(metadata, CmafMetadata::Text(_)) {
            return Err(Error::UnsupportedTrack);
        }

        let mut rules = self.rules().iter().filter(|rule| rule.matches(metadata));
        let rule = rules.next().ok_or(Error::NoMatchingRule)?;
        if rules.next().is_some() {
            return Err(Error::AmbiguousRule);
        }

        let key = self
            .keys()
            .iter()
            .find(|key| key.kid == rule.kid)
            .ok_or(Error::MissingKey(rule.kid))?;

        Ok(EncryptionConfig {
            scheme: key.common_encryption_scheme.parse()?,
            kid: key.kid,
            key: key.key()?,
        })
    }
}

impl std::str::FromStr for EncryptionScheme {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "cenc" => Ok(Self::Cenc),
            "cbcs" => Ok(Self::Cbcs),
            _ => Err(Error::UnsupportedScheme(value.to_string())),
        }
    }
}

impl ContentKeyUsageRule {
    fn matches(&self, metadata: &CmafMetadata) -> bool {
        match metadata {
            CmafMetadata::Audio(_) => self.audio_filter.is_some(),
            CmafMetadata::Video(video) => {
                let pixels = u64::from(video.width) * u64::from(video.height);
                self.video_filter.as_ref().is_some_and(|filter| {
                    filter.min_pixels.is_none_or(|min| pixels >= min)
                        && filter.max_pixels.is_none_or(|max| pixels <= max)
                })
            }
            CmafMetadata::Text(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use dyndo_core::track::cmaf::CmafMetadata;
    use dyndo_core::track::metadata::VideoMetadata;
    use uuid::uuid;

    use super::*;
    use crate::cpix_parser::CpixParser;

    #[test]
    fn resolves_hd_video_encryption_config() {
        let cpix = CpixParser::parse(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/cpix.xml"
        )))
        .unwrap();
        let metadata = CmafMetadata::Video(VideoMetadata {
            width: 1_920,
            height: 1_080,
            frame_rate: "25/1".to_string(),
        });

        let config = cpix.encryption_config_for(&metadata).unwrap();

        assert_eq!(
            config,
            EncryptionConfig {
                scheme: EncryptionScheme::Cenc,
                kid: uuid!("6d76f25c-b17f-5e16-b8ea-ef6bbf582d8e"),
                key: [
                    0xcb, 0x54, 0x10, 0x84, 0xc9, 0x97, 0x31, 0xae, 0xf4, 0xff, 0xf7, 0x45, 0x00,
                    0xc3, 0xae, 0xad,
                ],
            }
        );
    }
}
