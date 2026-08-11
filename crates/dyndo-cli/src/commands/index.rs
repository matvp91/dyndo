use clap::Args;
use dyndo_core::asset::Asset;
use dyndo_core::role::Role;
use dyndo_core::track::ResolvedSourceTrack;
use language_tags::LanguageTag;
use opendal::Operator;
use relative_path::{RelativePath, RelativePathBuf};

#[derive(Args)]
pub(crate) struct IndexArgs {
    /// Tracks: `<path>[,language=..][,role=..]`, one per track.
    #[arg(required = true, value_parser = parse_track_input)]
    inputs: Vec<TrackInput>,
    /// Output asset path.
    #[arg(short, long = "output", default_value = "asset.json")]
    output: String,
}

pub(crate) async fn run(op: &Operator, args: IndexArgs) -> Result<(), Box<dyn std::error::Error>> {
    let output_path = RelativePathBuf::from(args.output.as_str());
    let output_base = output_path.parent().unwrap_or(RelativePath::new(""));
    let mut asset = Asset::read_or_new(op, &output_path).await?;

    for input in args.inputs {
        let path = output_base.join(&input.path);
        if let Some(track) = asset.find_track_by_path(&path) {
            input.apply(track);
            continue;
        }

        let track = ResolvedSourceTrack::probe(op, &path, None).await?;
        input.apply(asset.add_source_track(&track));
    }

    asset.write(op).await?;
    println!("wrote {} ({} tracks)", args.output, asset.tracks.len());
    Ok(())
}

#[derive(Debug, Clone)]
struct TrackInput {
    path: RelativePathBuf,
    language: Option<LanguageTag>,
    role: Option<Role>,
}

impl TrackInput {
    fn apply(&self, track: &mut dyndo_core::track::SourceTrack) {
        let Some((language, role)) = track.language_and_role_mut() else {
            return;
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
