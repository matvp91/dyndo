use std::str::FromStr;

use dyndo_core::asset_descriptor::TrackKind;
use dyndo_core::role::Role;
use relative_path::RelativePathBuf;

#[derive(Debug, Clone)]
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

#[cfg(test)]
mod tests {
    use dyndo_core::asset_descriptor::{AudioKind, TextKind, VideoKind};

    use super::*;

    #[test]
    fn parse_accepts_plain_path() {
        let input: TrackInput = "video.mp4".parse().unwrap();

        assert_eq!(input.path, RelativePathBuf::from("video.mp4"));
    }

    #[test]
    fn parse_accepts_language_and_role() {
        let input: TrackInput = "audio.mp4,language=fra,role=commentary".parse().unwrap();

        assert_eq!(
            (input.language.as_deref(), input.role),
            (Some("fra"), Some(Role::Commentary))
        );
    }

    #[test]
    fn parse_rejects_unknown_field() {
        let error = "audio.mp4,codec=aac".parse::<TrackInput>().unwrap_err();

        assert_eq!(error, "expected language=.. or role=.., got \"codec=aac\"");
    }

    #[test]
    fn parse_treats_path_key_syntax_as_literal_path() {
        let input = "path=video.mp4".parse::<TrackInput>().unwrap();

        assert_eq!(input.path, RelativePathBuf::from("path=video.mp4"));
    }

    #[test]
    fn parse_rejects_unknown_role() {
        let error = "audio.mp4,role=unknown".parse::<TrackInput>().unwrap_err();

        assert_eq!(error, "unknown role: unknown");
    }

    #[test]
    fn apply_overrides_audio_language_and_role() {
        let input: TrackInput = "audio.mp4,language=fra,role=commentary".parse().unwrap();
        let mut kind = audio_kind();

        input.apply(&mut kind);

        let TrackKind::Audio(audio) = kind else {
            panic!("expected audio");
        };
        assert_eq!(
            (audio.language, audio.role),
            ("fra".to_string(), Some(Role::Commentary))
        );
    }

    #[test]
    fn apply_overrides_text_language_and_role() {
        let input: TrackInput = "text.vtt,language=nld,role=subtitle".parse().unwrap();
        let mut kind = TrackKind::Text(TextKind {
            language: "und".to_string(),
            role: None,
        });

        input.apply(&mut kind);

        let TrackKind::Text(text) = kind else {
            panic!("expected text");
        };
        assert_eq!(
            (text.language, text.role),
            ("nld".to_string(), Some(Role::Subtitle))
        );
    }

    #[test]
    fn apply_ignores_video_overrides() {
        let input: TrackInput = "video.mp4,language=fra".parse().unwrap();
        let mut kind = TrackKind::Video(VideoKind {
            width: 1920,
            height: 1080,
            frame_rate: "25/1".to_string(),
        });
        let expected = kind.clone();

        input.apply(&mut kind);

        assert_eq!(kind, expected);
    }

    fn audio_kind() -> TrackKind {
        TrackKind::Audio(AudioKind {
            sample_rate: 48_000,
            channels: 2,
            language: "nld".to_string(),
            role: None,
        })
    }
}
