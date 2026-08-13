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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionScheme {
    Cenc,
    Cbcs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptionConfig {
    pub scheme: EncryptionScheme,
    pub kid: Uuid,
    pub key: [u8; 16],
    pub drm_systems: Vec<DrmSystemConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrmSystemConfig {
    pub system_id: Uuid,
    pub pssh: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackMetadata {
    Audio,
    Video { width: u32, height: u32 },
}

impl Cpix {
    pub fn encryption_config_for(
        &self,
        metadata: TrackMetadata,
    ) -> Result<EncryptionConfig, Error> {
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
            drm_systems: self
                .drm_systems()
                .iter()
                .filter(|system| system.kid == key.kid)
                .map(|system| {
                    Ok(DrmSystemConfig {
                        system_id: system.system_id,
                        pssh: system.pssh()?,
                    })
                })
                .collect::<Result<_, crate::cpix_parser::Error>>()?,
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
    fn matches(&self, metadata: TrackMetadata) -> bool {
        match metadata {
            TrackMetadata::Audio => self.audio_filter.is_some(),
            TrackMetadata::Video { width, height } => {
                let pixels = u64::from(width) * u64::from(height);
                self.video_filter.as_ref().is_some_and(|filter| {
                    filter.min_pixels.is_none_or(|min| pixels >= min)
                        && filter.max_pixels.is_none_or(|max| pixels <= max)
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
        let metadata = TrackMetadata::Video {
            width: 1_920,
            height: 1_080,
        };

        let config = cpix.encryption_config_for(metadata).unwrap();

        assert_eq!(config.scheme, EncryptionScheme::Cenc);
        assert_eq!(config.kid, uuid!("6d76f25c-b17f-5e16-b8ea-ef6bbf582d8e"));
    }
}
