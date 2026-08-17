use clap::Args;
use dyndo_core::{
    asset::{Asset, AssetError},
    role::Role,
    track::{DiscoveredCmafTrack, SidecarTextTrack, TextMetadata, TextTrack, Track},
};
use language_tags::LanguageTag;
use opendal::ErrorKind;
use relative_path::{RelativePath, RelativePathBuf};

#[derive(Args)]
pub(crate) struct IndexArgs {
    /// Tracks: `<path>[,language=..][,role=..]`, one per track.
    #[arg(required = true, value_parser = parse_track_input)]
    inputs: Vec<TrackInput>,
    /// Output asset path.
    #[arg(short, long, default_value = "asset.json")]
    output: String,
}

pub(crate) async fn run(args: IndexArgs) -> Result<(), Box<dyn std::error::Error>> {
    let output_path = RelativePathBuf::from(args.output.as_str());
    let output_base = output_path.parent().unwrap_or(RelativePath::new(""));
    let mut asset = read_or_default(&output_path).await?;

    for input in args.inputs {
        let source_path = output_base.join(&input.path);
        if let Some(index) = asset
            .tracks
            .iter()
            .position(|track| asset.track_path(track).as_ref() == Some(&source_path))
        {
            input.apply(&mut asset.tracks[index]);
            continue;
        }

        let mut track = discover_track(source_path.as_relative_path(), input.path.clone()).await?;
        input.apply(&mut track);
        asset.tracks.push(track);
    }

    asset.write().await?;
    println!("wrote {} ({} tracks)", args.output, asset.tracks.len());

    Ok(())
}

async fn read_or_default(path: &RelativePath) -> Result<Asset, Box<dyn std::error::Error>> {
    match Asset::read(path).await {
        Ok(asset) => Ok(asset),
        Err(AssetError::Read(error)) if error.kind() == ErrorKind::NotFound => {
            Ok(Asset::new(path.to_owned()))
        }
        Err(error) => Err(error.into()),
    }
}

async fn discover_track(
    source_path: &RelativePath,
    asset_relative_path: RelativePathBuf,
) -> Result<Track, Box<dyn std::error::Error>> {
    match source_path.extension() {
        Some("mp4") => Ok(DiscoveredCmafTrack::discover(source_path)
            .await?
            .into_track(asset_relative_path)),
        Some("vtt") | Some("imsc") => Ok(Track::Text(TextTrack::Sidecar(SidecarTextTrack {
            path: asset_relative_path,
            metadata: TextMetadata {
                language: "und".parse()?,
                role: None,
            },
        }))),
        _ => Err(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "unsupported track format")
                .into(),
        ),
    }
}

#[derive(Debug, Clone)]
struct TrackInput {
    path: RelativePathBuf,
    language: Option<LanguageTag>,
    role: Option<Role>,
}

impl TrackInput {
    fn apply(&self, track: &mut Track) {
        match track {
            Track::Audio(track) => {
                self.apply_language_and_role(
                    &mut track.metadata.language,
                    &mut track.metadata.role,
                );
            }
            Track::Text(TextTrack::Cmaf(track)) => {
                self.apply_language_and_role(
                    &mut track.metadata.language,
                    &mut track.metadata.role,
                );
            }
            Track::Text(TextTrack::Sidecar(track)) => {
                self.apply_language_and_role(
                    &mut track.metadata.language,
                    &mut track.metadata.role,
                );
            }
            Track::Video(_) | Track::Thumbnail(_) => {}
        }
    }

    fn apply_language_and_role(&self, language: &mut LanguageTag, role: &mut Option<Role>) {
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
                        .parse()
                        .map_err(|_| format!("invalid language tag: {value}"))?,
                );
            }
            Some(("role", value)) => {
                role = Some(
                    value
                        .parse()
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
