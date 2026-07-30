//! Parsing and applying `dyndo index` track descriptors:
//! `<path>[,language=..][,role=..]`.

use dyndo_core::next::role::Role;
use dyndo_core::next::track::Track;
use dyndo_core::next::track_metadata::Kind;

/// Parse an `index` track descriptor `<path>[,language=..][,role=..]` into
/// its path and overrides. An empty value (`language=`) means "unset".
pub fn parse_track_descriptor(
    input: &str,
) -> Result<(String, Option<String>, Option<String>), String> {
    let mut fields = input.split(',');
    // `split` always yields at least one item; the first is the file path.
    let path = fields.next().unwrap_or_default().to_string();
    let (mut language, mut role) = (None, None);
    for field in fields {
        match field.split_once('=') {
            Some(("language", v)) => language = (!v.is_empty()).then(|| v.to_string()),
            Some(("role", v)) => role = (!v.is_empty()).then(|| v.to_string()),
            _ => return Err(format!("expected language=.. or role=.., got {field:?}")),
        }
    }
    Ok((path, language, role))
}

/// Apply a track descriptor's `language` and `role` overrides. Video tracks
/// have no language; roles apply to every track kind.
pub fn apply_overrides(
    track: &mut Track,
    language: Option<&str>,
    role: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(language) = language {
        match &mut track.metadata.kind {
            Kind::Video(_) => {
                return Err(format!("{}: video tracks take no language", track.path).into());
            }
            Kind::Audio(audio) => audio.language = language.to_string(),
            Kind::Text(text) => text.language = language.to_string(),
        }
    }
    if let Some(role) = role {
        track.metadata.role = Some(parse_role::<Role>(role)?);
    }
    Ok(())
}

/// Parse a kebab-case role string through the role's serde vocabulary, so the
/// CLI accepts exactly the values descriptors do.
fn parse_role<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, String> {
    serde_json::from_value(serde_json::Value::String(s.to_string()))
        .map_err(|_| format!("unknown role: {s}"))
}
