use base64::prelude::{BASE64_STANDARD, Engine as _};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("malformed CPIX document")]
    Xml(#[from] quick_xml::DeError),
    #[error("content key is not valid base64")]
    Base64(#[from] base64::DecodeError),
    #[error("content key is {0} bytes, expected 16")]
    KeyLength(usize),
}

pub struct CpixParser;

impl CpixParser {
    pub fn parse(xml: &str) -> Result<Cpix, Error> {
        Ok(quick_xml::de::from_str(xml)?)
    }

    pub fn parse_bytes(xml: &[u8]) -> Result<Cpix, Error> {
        Ok(quick_xml::de::from_reader(xml)?)
    }
}

#[derive(Debug, Deserialize)]
pub struct Cpix {
    #[serde(rename = "@contentId")]
    pub content_id: Option<String>,
    #[serde(rename = "ContentKeyList", alias = "cpix:ContentKeyList", default)]
    content_key_list: ContentKeyList,
    #[serde(
        rename = "ContentKeyUsageRuleList",
        alias = "cpix:ContentKeyUsageRuleList",
        default
    )]
    usage_rule_list: ContentKeyUsageRuleList,
}

impl Cpix {
    pub fn keys(&self) -> &[ContentKey] {
        &self.content_key_list.keys
    }

    pub fn rules(&self) -> &[ContentKeyUsageRule] {
        &self.usage_rule_list.rules
    }
}

#[derive(Debug, Default, Deserialize)]
struct ContentKeyList {
    #[serde(rename = "ContentKey", alias = "cpix:ContentKey", default)]
    keys: Vec<ContentKey>,
}

#[derive(Debug, Default, Deserialize)]
struct ContentKeyUsageRuleList {
    #[serde(
        rename = "ContentKeyUsageRule",
        alias = "cpix:ContentKeyUsageRule",
        default
    )]
    rules: Vec<ContentKeyUsageRule>,
}

#[derive(Debug, Deserialize)]
pub struct ContentKey {
    #[serde(rename = "@kid")]
    pub kid: Uuid,
    #[serde(rename = "@commonEncryptionScheme")]
    pub common_encryption_scheme: String,
    #[serde(rename = "Data", alias = "cpix:Data")]
    data: Data,
}

impl ContentKey {
    pub fn key(&self) -> Result<[u8; 16], Error> {
        let bytes = BASE64_STANDARD.decode(self.data.secret.plain_value.trim())?;
        let len = bytes.len();
        bytes.try_into().map_err(|_| Error::KeyLength(len))
    }
}

#[derive(Debug, Deserialize)]
struct Data {
    #[serde(rename = "Secret", alias = "pskc:Secret")]
    secret: Secret,
}

#[derive(Debug, Deserialize)]
struct Secret {
    #[serde(rename = "PlainValue", alias = "pskc:PlainValue")]
    plain_value: String,
}

#[derive(Debug, Deserialize)]
pub struct ContentKeyUsageRule {
    #[serde(rename = "@kid")]
    pub kid: Uuid,
    #[serde(rename = "@intendedTrackType")]
    pub intended_track_type: Option<String>,
    #[serde(rename = "AudioFilter", alias = "cpix:AudioFilter")]
    pub audio_filter: Option<AudioFilter>,
    #[serde(rename = "VideoFilter", alias = "cpix:VideoFilter")]
    pub video_filter: Option<VideoFilter>,
}

#[derive(Debug, Deserialize)]
pub struct AudioFilter {}

#[derive(Debug, Deserialize)]
pub struct VideoFilter {
    #[serde(rename = "@minPixels")]
    pub min_pixels: Option<u64>,
    #[serde(rename = "@maxPixels")]
    pub max_pixels: Option<u64>,
}

#[cfg(test)]
mod tests {
    use uuid::uuid;

    use super::*;

    #[test]
    fn parses_demo_document() {
        let xml = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/cpix.xml"
        ));
        let cpix = CpixParser::parse(xml).unwrap();

        assert_eq!(cpix.content_id.as_deref(), Some("test-content"));

        let keys = cpix.keys();
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0].kid, uuid!("f3c5e036-1e66-54b2-8f80-49c778b23946"));
        assert_eq!(keys[0].common_encryption_scheme, "cenc");
        assert_eq!(
            keys[0].key().unwrap(),
            [
                0xa4, 0x63, 0x1a, 0x15, 0x3a, 0x44, 0x3d, 0xf9, 0xee, 0xd0, 0x59, 0x30, 0x43, 0xdb,
                0x75, 0x19
            ]
        );

        let rules = cpix.rules();
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[0].kid, keys[0].kid);
        assert_eq!(rules[0].intended_track_type.as_deref(), Some("AUDIO"));
        assert!(rules[0].audio_filter.is_some());
        assert!(rules[0].video_filter.is_none());

        let hd = rules[2].video_filter.as_ref().unwrap();
        assert_eq!(hd.min_pixels, Some(442_369));
        assert_eq!(hd.max_pixels, Some(2_073_600));
    }
}
