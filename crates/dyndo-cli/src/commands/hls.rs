use clap::Args;
use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::track::Track;
use opendal::Operator;
use relative_path::RelativePathBuf;

use super::{SegmentArgs, parse_filter};

#[derive(Args)]
pub(crate) struct HlsArgs {
    /// Input asset.json path.
    #[arg(short, long = "input")]
    input: String,
    /// Output playlist directory.
    #[arg(short, long = "output", default_value = "hls")]
    output: String,
    #[command(flatten)]
    segment: SegmentArgs,
    /// Point text renditions at packaged wvtt segments rather than WebVTT
    /// documents.
    #[arg(long, default_value_t = false)]
    wvtt: bool,
    /// Write playlists only for the tracks this expression keeps, for example
    /// `--filter 'type!=video||height<=720'`.
    #[arg(long)]
    filter: Option<String>,
}

pub(super) async fn run(op: &Operator, args: HlsArgs) -> Result<(), Box<dyn std::error::Error>> {
    let mut descriptor = AssetDescriptor::read(op, &args.input).await?;
    args.segment.assign_to(&mut descriptor.segment_options);
    // Narrowed here rather than handed to the builder, because a media playlist is
    // written per track below: the descriptor the loop walks has to be the same one
    // the multivariant playlist describes, or it would emit playlists nothing
    // references.
    if let Some(filter) = parse_filter(args.filter.as_deref())? {
        let tracks = Track::probe_all(op, &descriptor).await?;
        (descriptor, _) = filter.narrow(&descriptor, tracks)?;
    }
    let output = RelativePathBuf::from(args.output);
    op.create_dir(&format!("{output}/")).await?;

    let hls_options = dyndo_hls::options::HlsOptions { wvtt: args.wvtt };
    let master =
        dyndo_hls::builder::generate_master_playlist(op, &descriptor, &hls_options, None).await?;
    let master_path = output.join("master.m3u8");
    op.write(master_path.as_str(), master.to_string()).await?;
    println!("wrote {master_path}");

    for track in &descriptor.tracks {
        let playlist =
            dyndo_hls::builder::generate_media_playlist(op, &descriptor, track, &hls_options)
                .await?;
        let path = output.join(format!("{}.m3u8", track.id));
        op.write(
            path.as_str(),
            dyndo_hls::builder::serialize_media_playlist(&playlist),
        )
        .await?;
        println!("wrote {path}");
    }
    Ok(())
}
