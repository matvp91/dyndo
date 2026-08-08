use clap::Args;
use dyndo_core::asset_descriptor::AssetDescriptor;
use opendal::Operator;
use serde::Serialize;

use super::{SegmentArgs, parse_filter};

#[derive(Args)]
pub(crate) struct DashArgs {
    /// Input asset.json path.
    #[arg(short, long = "input")]
    input: String,
    /// Output manifest path.
    #[arg(short, long = "output", default_value = "stream.mpd")]
    output: String,
    #[command(flatten)]
    segment: SegmentArgs,
    /// Hoist common segment information in the MPD.
    #[arg(long, default_value_t = false)]
    compact: bool,
    /// Split the manifest into a Period at each segment boundary.
    #[arg(long = "multi-period", default_value_t = false)]
    multi_period: bool,
    /// Describe only the tracks this expression keeps, for example
    /// `--filter 'type!=video||height<=720'`.
    #[arg(long)]
    filter: Option<String>,
}

pub(super) async fn run(op: &Operator, args: DashArgs) -> Result<(), Box<dyn std::error::Error>> {
    let mut descriptor = AssetDescriptor::read(op, &args.input).await?;
    args.segment.assign_to(&mut descriptor.segment_options);

    let filter = parse_filter(args.filter.as_deref())?;

    // Thumbnails are served, not written: the CLI has nowhere to point a sprite
    // template at, so it describes none.
    let dash_options = dyndo_dash::options::DashOptions {
        compact: args.compact,
        multi_period: args.multi_period,
        ..Default::default()
    };

    let mpd =
        dyndo_dash::builder::generate_mpd(op, &descriptor, &dash_options, filter.as_ref()).await?;

    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let mut serializer = quick_xml::se::Serializer::new(&mut xml);
    serializer.indent(' ', 2);
    mpd.serialize(serializer)?;
    op.write(&args.output, xml.into_bytes()).await?;
    println!("wrote {}", args.output);
    Ok(())
}
