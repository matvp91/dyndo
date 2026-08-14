//! CPIX parsing and resolved content-protection policy.

mod cpix;

use cpix::ContentKeyUsageRule;
pub use cpix::{Cpix, CpixParser, Error as CpixError};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Key(#[from] cpix::Error),
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

impl EncryptionScheme {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cenc => "cenc",
            Self::Cbcs => "cbcs",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Protection {
    pub(crate) scheme: EncryptionScheme,
    pub(crate) kid: Uuid,
    pub(crate) systems: Vec<ProtectionSystem>,
}

impl Protection {
    pub const fn scheme(&self) -> EncryptionScheme {
        self.scheme
    }

    pub const fn key_id(&self) -> Uuid {
        self.kid
    }

    pub fn systems(&self) -> &[ProtectionSystem] {
        &self.systems
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectionSystem {
    pub(crate) system_id: Uuid,
    pub(crate) pssh: Vec<u8>,
}

impl ProtectionSystem {
    pub const fn system_id(&self) -> Uuid {
        self.system_id
    }

    pub fn pssh(&self) -> &[u8] {
        &self.pssh
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EncryptionConfig {
    pub(crate) protection: Protection,
    pub(crate) key: [u8; 16],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrackMetadata {
    Audio,
    Video { width: u32, height: u32 },
}

impl Cpix {
    pub(crate) fn encryption_config_for(
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
            protection: Protection {
                scheme: key.common_encryption_scheme.parse()?,
                kid: key.kid,
                systems: self
                    .drm_systems()
                    .iter()
                    .filter(|system| system.kid == key.kid)
                    .map(|system| {
                        Ok(ProtectionSystem {
                            system_id: system.system_id,
                            pssh: system.pssh()?,
                        })
                    })
                    .collect::<Result<_, cpix::Error>>()?,
            },
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

    use super::CpixParser;
    use super::*;

    #[test]
    fn resolves_hd_video_encryption_config() {
        let cpix = CpixParser::parse(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/cpix_mk.xml"
        )))
        .unwrap();
        let metadata = TrackMetadata::Video {
            width: 1_920,
            height: 1_080,
        };

        let config = cpix.encryption_config_for(metadata).unwrap();

        assert_eq!(config.protection.scheme(), EncryptionScheme::Cenc);
        assert_eq!(
            config.protection.key_id(),
            uuid!("6d76f25c-b17f-5e16-b8ea-ef6bbf582d8e")
        );
    }
}
