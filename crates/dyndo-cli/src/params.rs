//! Parsing and applying `dyndo index` track-descriptor parameters:
//! `<path>[,language=..][,role=..]`.

use dyndo_core::metadata::Metadata;
use dyndo_core::role::{AudioRole, TextRole};
use dyndo_core::track::Track;

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

/// Apply a track descriptor's `language`/`role` overrides onto the track's
/// metadata. Video tracks take neither.
pub fn apply_overrides(
    track: &mut Track,
    language: Option<&str>,
    role: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    match &mut track.metadata {
        Metadata::Video(_) => {
            if language.is_some() || role.is_some() {
                return Err(
                    format!("{}: video tracks take no language or role", track.path).into(),
                );
            }
        }
        Metadata::Audio(a) => {
            if let Some(l) = language {
                a.language = l.to_string();
            }
            if let Some(r) = role {
                a.role = Some(parse_role::<AudioRole>(r)?);
            }
        }
        Metadata::Text(t) => {
            if let Some(l) = language {
                t.language = l.to_string();
            }
            if let Some(r) = role {
                t.role = Some(parse_role::<TextRole>(r)?);
            }
        }
    }
    Ok(())
}

/// Parse a kebab-case role string through the role's serde vocabulary, so the
/// CLI accepts exactly the values descriptors do.
fn parse_role<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, String> {
    serde_json::from_value(serde_json::Value::String(s.to_string()))
        .map_err(|_| format!("unknown role: {s}"))
}
