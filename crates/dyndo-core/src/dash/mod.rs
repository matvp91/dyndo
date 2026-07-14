//! DASH MPD generation for dyndo assets.

mod build;
mod compact;

use serde::Serialize;

use crate::asset::Asset;

/// Build a static DASH MPD from `asset`, pretty-printed with two-space
/// indentation. When `compact` is set, `SegmentTemplate` content shared by all
/// Representations is hoisted to the `AdaptationSet` level.
///
/// # Panics
/// Panics only if MPD serialization fails, which cannot happen for a
/// well-formed model.
pub fn generate_mpd(asset: &Asset, compact: bool) -> String {
    let mpd = build::build_mpd(&asset.tracks, compact);

    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let mut serializer = quick_xml::se::Serializer::new(&mut xml);
    serializer.indent(' ', 2);
    mpd.serialize(serializer)
        .expect("MPD serialization is infallible for a well-formed model");
    xml
}
