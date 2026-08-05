use std::str::FromStr;

use dyndo_core::asset_descriptor::TrackKind;
use dyndo_core::role::Role;
use relative_path::RelativePathBuf;

#[derive(Clone)]
pub(crate) struct TrackInput {
    pub(crate) path: RelativePathBuf,
    language: Option<String>,
    role: Option<Role>,
}

impl TrackInput {
    pub(crate) fn apply(&self, kind: &mut TrackKind) {
        match kind {
            TrackKind::Audio(audio) => {
                if let Some(language) = &self.language {
                    audio.language.clone_from(language);
                }
                if let Some(role) = self.role {
                    audio.role = Some(role);
                }
            }
            TrackKind::Text(text) => {
                if let Some(language) = &self.language {
                    text.language.clone_from(language);
                }
                if let Some(role) = self.role {
                    text.role = Some(role);
                }
            }
            TrackKind::Video(_) => {}
        }
    }
}

impl FromStr for TrackInput {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut fields = input.split(',');
        let path = RelativePathBuf::from(fields.next().unwrap_or_default());
        let (mut language, mut role) = (None, None);

        for field in fields {
            match field.split_once('=') {
                Some(("language", value)) => {
                    language = (!value.is_empty()).then(|| value.to_string());
                }
                Some(("role", value)) => {
                    role = if value.is_empty() {
                        None
                    } else {
                        Some(
                            serde_json::from_value(serde_json::Value::String(value.to_string()))
                                .map_err(|_| format!("unknown role: {value}"))?,
                        )
                    };
                }
                _ => return Err(format!("expected language=.. or role=.., got {field:?}")),
            }
        }

        Ok(Self {
            path,
            language,
            role,
        })
    }
}
