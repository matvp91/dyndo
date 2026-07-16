//! Parse a `dyndo index` track descriptor: `<path>[,key=value]...`.
//!
//! Field 0 is always the file path; `language` and `role` are the only keys.
//! This layer validates syntax only — it does not probe, so audio-vs-text and
//! role-value validation happen later in `Asset::upsert_track`.

/// A parsed `index` input: a source path plus optional per-track overrides.
pub struct TrackDescriptor {
    pub path: String,
    pub language: Option<String>,
    pub role: Option<String>,
}

impl TrackDescriptor {
    /// Parse `<path>[,language=..][,role=..]`. The first comma-field is the
    /// file path (no key); the rest are `key=value` with keys `language` and
    /// `role`. An empty value (`language=`) means "unset".
    pub fn parse(s: &str) -> Result<TrackDescriptor, String> {
        let mut fields = s.split(',');
        // `split` always yields at least one item.
        let path = fields.next().unwrap_or_default();
        if path.is_empty() {
            return Err("empty descriptor: the first field must be the file path".to_string());
        }
        if path.contains('=') {
            return Err(format!(
                "the first field is the file path, not a key=value: {path:?}"
            ));
        }

        let mut language = None;
        let mut role = None;
        for field in fields {
            let (key, value) = field
                .split_once('=')
                .ok_or_else(|| format!("expected key=value, got {field:?}"))?;
            match key {
                "language" => {
                    if language.is_some() {
                        return Err("duplicate field: language".to_string());
                    }
                    language = (!value.is_empty()).then(|| value.to_string());
                }
                "role" => {
                    if role.is_some() {
                        return Err("duplicate field: role".to_string());
                    }
                    role = (!value.is_empty()).then(|| value.to_string());
                }
                "path" => {
                    return Err("path is the first field, not a key".to_string());
                }
                other => {
                    return Err(format!("unknown field {other:?} (valid: language, role)"));
                }
            }
        }
        Ok(TrackDescriptor {
            path: path.to_string(),
            language,
            role,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_bare_path() {
        let d = TrackDescriptor::parse("video.mp4").unwrap();
        assert_eq!(d.path, "video.mp4");
        assert_eq!(d.language, None);
        assert_eq!(d.role, None);
    }

    #[test]
    fn parses_path_with_language_and_role() {
        let d = TrackDescriptor::parse("audio.mp4,language=nld,role=main").unwrap();
        assert_eq!(d.path, "audio.mp4");
        assert_eq!(d.language.as_deref(), Some("nld"));
        assert_eq!(d.role.as_deref(), Some("main"));
    }

    #[test]
    fn empty_language_value_is_unset() {
        let d = TrackDescriptor::parse("audio.mp4,language=").unwrap();
        assert_eq!(d.language, None);
    }

    #[test]
    fn rejects_unknown_key() {
        assert!(TrackDescriptor::parse("audio.mp4,codec=aac").is_err());
    }

    #[test]
    fn rejects_a_field_without_equals() {
        assert!(TrackDescriptor::parse("audio.mp4,language").is_err());
    }

    #[test]
    fn rejects_empty_path() {
        assert!(TrackDescriptor::parse("").is_err());
        assert!(TrackDescriptor::parse(",language=nld").is_err());
    }

    #[test]
    fn rejects_path_as_a_key() {
        // The path is field 0; `path=` as the first field is a mistake.
        assert!(TrackDescriptor::parse("path=video.mp4").is_err());
    }

    #[test]
    fn rejects_a_duplicate_key() {
        assert!(TrackDescriptor::parse("audio.mp4,role=main,role=dub").is_err());
    }
}
