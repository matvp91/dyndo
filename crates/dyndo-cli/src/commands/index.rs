use clap::Args;
use dyndo_core::asset_descriptor::{AssetDescriptor, TrackKind};
use dyndo_core::role::Role;
use dyndo_core::track::Track;
use language_tags::LanguageTag;
use opendal::Operator;
use relative_path::{RelativePath, RelativePathBuf};

#[derive(Args)]
pub(crate) struct IndexArgs {
    /// Track descriptor(s): `<path>[,language=..][,role=..]`, one per track.
    #[arg(required = true, value_parser = parse_track_input)]
    inputs: Vec<TrackInput>,
    /// Output descriptor path.
    #[arg(short, long = "output", default_value = "asset.json")]
    output: String,
}

pub(super) async fn run(op: &Operator, args: IndexArgs) -> Result<(), Box<dyn std::error::Error>> {
    let output_path = RelativePathBuf::from(args.output.as_str());
    let output_base = output_path.parent().unwrap_or(RelativePath::new(""));
    let mut descriptor = AssetDescriptor::read_or_new(op, &output_path).await?;

    for input in args.inputs {
        let path = output_base.join(&input.path);
        if let Some(track) = descriptor.find_track_mut(&path) {
            input.apply(&mut track.kind);
            continue;
        }

        let track = Track::probe(op, &path, None, &descriptor.segment_options).await?;
        input.apply(&mut descriptor.add_track(&track).kind);
    }

    op.write(&args.output, serde_json::to_vec_pretty(&descriptor)?)
        .await?;
    println!("wrote {} ({} tracks)", args.output, descriptor.tracks.len());
    Ok(())
}

/// One `<path>[,language=..][,role=..]` argument: the track to index, plus the
/// metadata a probe cannot read off the file itself.
#[derive(Debug, Clone)]
struct TrackInput {
    path: RelativePathBuf,
    language: Option<LanguageTag>,
    role: Option<Role>,
}

impl TrackInput {
    /// Applies the overrides this input names onto `kind`, leaving the rest as the
    /// probe read them.
    fn apply(&self, kind: &mut TrackKind) {
        let (language, role) = match kind {
            TrackKind::Audio(audio) => (&mut audio.language, &mut audio.role),
            TrackKind::Text(text) => (&mut text.language, &mut text.role),
            TrackKind::Video(_) => return,
        };
        if let Some(value) = &self.language {
            language.clone_from(value);
        }
        if let Some(value) = self.role {
            *role = Some(value);
        }
    }
}

fn parse_track_input(input: &str) -> Result<TrackInput, String> {
    let mut fields = input.split(',');
    let path = RelativePathBuf::from(fields.next().unwrap());
    let (mut language, mut role) = (None, None);

    for field in fields {
        match field.split_once('=') {
            Some(("language", value)) => {
                language = Some(
                    value
                        .parse::<LanguageTag>()
                        .map_err(|_| format!("invalid language tag: {value}"))?,
                );
            }
            Some(("role", value)) => {
                role = Some(
                    serde_json::from_value(serde_json::Value::String(value.to_string()))
                        .map_err(|_| format!("unknown role: {value}"))?,
                );
            }
            _ => return Err(format!("expected language=.. or role=.., got {field:?}")),
        }
    }

    Ok(TrackInput {
        path,
        language,
        role,
    })
}

#[cfg(test)]
mod tests {
    use dyndo_core::asset_descriptor::{AudioKind, TextKind, VideoKind};

    use super::*;

    #[test]
    fn parse_accepts_plain_path() {
        let input = parse_track_input("video.mp4").unwrap();

        assert_eq!(input.path, RelativePathBuf::from("video.mp4"));
    }

    #[test]
    fn parse_accepts_language_and_role() {
        let input = parse_track_input("audio.mp4,language=fra,role=commentary").unwrap();

        assert_eq!(
            (input.language.as_ref().map(LanguageTag::as_str), input.role),
            (Some("fra"), Some(Role::Commentary))
        );
    }

    #[test]
    fn parse_accepts_bcp47_language_tag() {
        let input = parse_track_input("audio.mp4,language=pt-BR").unwrap();

        assert_eq!(
            input.language.as_ref().map(LanguageTag::as_str),
            Some("pt-BR")
        );
    }

    #[test]
    fn parse_rejects_malformed_language_tag() {
        let error = parse_track_input("audio.mp4,language=not_a_tag").unwrap_err();

        assert_eq!(error, "invalid language tag: not_a_tag");
    }

    #[test]
    fn parse_rejects_empty_language_tag() {
        let error = parse_track_input("audio.mp4,language=").unwrap_err();

        assert_eq!(error, "invalid language tag: ");
    }

    #[test]
    fn parse_rejects_unknown_field() {
        let error = parse_track_input("audio.mp4,codec=aac").unwrap_err();

        assert_eq!(error, "expected language=.. or role=.., got \"codec=aac\"");
    }

    #[test]
    fn parse_treats_path_key_syntax_as_literal_path() {
        let input = parse_track_input("path=video.mp4").unwrap();

        assert_eq!(input.path, RelativePathBuf::from("path=video.mp4"));
    }

    #[test]
    fn parse_rejects_unknown_role() {
        let error = parse_track_input("audio.mp4,role=unknown").unwrap_err();

        assert_eq!(error, "unknown role: unknown");
    }

    #[test]
    fn apply_overrides_audio_language_and_role() {
        let input = parse_track_input("audio.mp4,language=fra,role=commentary").unwrap();
        let mut kind = audio_kind();

        input.apply(&mut kind);

        let TrackKind::Audio(audio) = kind else {
            panic!("expected audio");
        };
        assert_eq!(
            (audio.language, audio.role),
            ("fra".parse().unwrap(), Some(Role::Commentary))
        );
    }

    #[test]
    fn apply_overrides_text_language_and_role() {
        let input = parse_track_input("text.vtt,language=nld,role=subtitle").unwrap();
        let mut kind = TrackKind::Text(TextKind {
            language: "und".parse().unwrap(),
            role: None,
        });

        input.apply(&mut kind);

        let TrackKind::Text(text) = kind else {
            panic!("expected text");
        };
        assert_eq!(
            (text.language, text.role),
            ("nld".parse().unwrap(), Some(Role::Subtitle))
        );
    }

    #[test]
    fn apply_ignores_video_overrides() {
        let input = parse_track_input("video.mp4,language=fra").unwrap();
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
            language: "nld".parse().unwrap(),
            role: None,
        })
    }
}
