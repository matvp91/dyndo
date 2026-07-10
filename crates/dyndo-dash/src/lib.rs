//! DASH MPD generation for dyndo assets, built on `dyndo_core`.

mod build;
mod compact;

use dyndo_core::asset::Asset;
use serde::Serialize;

/// Build a static DASH MPD from `asset`, pretty-printed with two-space
/// indentation. When `compact` is set, `SegmentTemplate` content shared by all
/// Representations is hoisted to the `AdaptationSet` level.
pub fn generate_mpd(asset: &Asset, compact: bool) -> String {
    let mpd = build::build_mpd(&asset.tracks, compact);

    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let mut serializer = quick_xml::se::Serializer::new(&mut xml);
    serializer.indent(' ', 2);
    mpd.serialize(serializer)
        .expect("MPD serialization is infallible for a well-formed model");
    xml
}
