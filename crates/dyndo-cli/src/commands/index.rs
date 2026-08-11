use clap::Args;
use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::role::Role;
use dyndo_core::source_track::SourceTrack;
use dyndo_core::track_descriptor::TrackDescriptor;
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

pub(crate) async fn run(op: &Operator, args: IndexArgs) -> Result<(), Box<dyn std::error::Error>> {
    let output_path = RelativePathBuf::from(args.output.as_str());
    let output_base = output_path.parent().unwrap_or(RelativePath::new(""));
    let mut descriptor = AssetDescriptor::read_or_new(op, &output_path).await?;

    for input in args.inputs {
        let path = output_base.join(&input.path);
        if let Some(track) = descriptor.find_track_by_path(&path) {
            input.apply(track);
            continue;
        }

        let track = SourceTrack::probe(op, &path, None).await?;
        input.apply(descriptor.add_source_track(&track));
    }

    op.write(&args.output, serde_json::to_vec_pretty(&descriptor)?)
        .await?;
    println!("wrote {} ({} tracks)", args.output, descriptor.tracks.len());
    Ok(())
}

#[derive(Debug, Clone)]
struct TrackInput {
    path: RelativePathBuf,
    language: Option<LanguageTag>,
    role: Option<Role>,
}

impl TrackInput {
    fn apply(&self, descriptor: &mut TrackDescriptor) {
        let (language, role) = match descriptor {
            TrackDescriptor::Audio(audio) => (&mut audio.kind.language, &mut audio.kind.role),
            TrackDescriptor::Text(text) => (&mut text.kind.language, &mut text.kind.role),
            TrackDescriptor::Vtt(text) => (&mut text.kind.language, &mut text.kind.role),
            TrackDescriptor::Video(_) | TrackDescriptor::Thumbnail(_) => return,
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
    let path = RelativePathBuf::from(fields.next().unwrap_or_default());
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
