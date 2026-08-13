use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use dyndo_core::drm::Protection;
use m3u8_rs::{ExtTag, Key, KeyMethod};

pub(crate) fn keys(protection: Option<&Protection>) -> Vec<Key> {
    let Some(protection) = protection else {
        return Vec::new();
    };
    protection
        .systems()
        .iter()
        .map(|system| Key {
            method: KeyMethod::Other("SAMPLE-AES-CTR".to_string()),
            uri: Some(format!(
                "data:text/plain;base64,{}",
                BASE64_STANDARD.encode(system.pssh())
            )),
            iv: None,
            keyformat: Some(format!("urn:uuid:{}", system.system_id())),
            keyformatversions: Some("1".to_string()),
        })
        .collect()
}

pub(crate) fn key_tag(key: Key) -> ExtTag {
    let uri = key.uri.as_deref().unwrap_or_default();
    let keyformat = key.keyformat.as_deref().unwrap_or_default();
    let versions = key.keyformatversions.as_deref().unwrap_or_default();
    ExtTag {
        tag: "KEY".to_string(),
        rest: Some(format!(
            "METHOD={},URI=\"{uri}\",KEYFORMAT=\"{keyformat}\",KEYFORMATVERSIONS=\"{versions}\"",
            key.method
        )),
    }
}
