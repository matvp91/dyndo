mod build;
mod compact;
mod timeline;

use std::path::Path;

pub(crate) use build::build_mpd;
use serde::Serialize;

use crate::asset::Asset;
use crate::cmaf::read_header;
use crate::error::{Error, Result};
use crate::storage::LocalFile;

/// Read every track's source (joined onto `base`), then build + serialize a static
/// DASH MPD, pretty-printed with two-space indentation. Each `track.source()` is
/// resolved against `base` at read time; `base` is normally the directory that
/// contains the `asset.json`, so sources stay relative to the descriptor. When
/// `compact` is set, `SegmentTemplate` content shared by all Representations is
/// hoisted to the `AdaptationSet` level.
pub async fn generate_mpd(asset: &Asset, base: &Path, compact: bool) -> Result<String> {
    let mut headers = Vec::with_capacity(asset.tracks.len());
    for track in &asset.tracks {
        let path = base.join(track.source());
        let key = path.to_string_lossy().into_owned();
        let source = LocalFile::new(&path);
        let header = read_header(&source, &key).await?;
        headers.push((track.id().to_string(), header));
    }

    let mpd = build_mpd(&headers, compact);

    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let mut serializer = quick_xml::se::Serializer::new(&mut xml);
    serializer.indent(' ', 2);
    mpd.serialize(serializer)
        .map_err(|e| Error::MpdSerialization(e.to_string()))?;
    Ok(xml)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name)
    }

    #[tokio::test]
    async fn generates_static_mpd_with_segment_timeline() {
        let asset: Asset = serde_json::from_value(serde_json::json!({
            "tracks": [
                { "type": "video", "id": "video_avc_1080_4807", "source": fixture("video_avc_1080.mp4"),
                  "fourcc": "avc1", "timescale": 90000, "width": 1920, "height": 1080 },
                { "type": "audio", "id": "audio_aac_nld_2_197", "source": fixture("audio_aac_nl_2.mp4"),
                  "fourcc": "mp4a", "timescale": 48000, "sample_rate": 48000, "channels": 2, "language": "nld" }
            ]
        }))
        .unwrap();

        let xml = generate_mpd(&asset, Path::new("."), false).await.unwrap();
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<MPD"));
        // pretty-printed: nested elements are indented on their own lines
        assert!(xml.contains("\n  <Period"));
        assert!(xml.contains("\n    <AdaptationSet"));
        assert!(xml.contains("type=\"static\""));
        assert!(xml.contains("<SegmentTimeline>"));
        assert!(xml.contains("codecs=\"avc1.640028\""));
        assert!(xml.contains("codecs=\"mp4a.40.2\""));
        assert!(xml.contains("$RepresentationID$/$Time$.m4s"));
        assert!(xml.contains("video_avc_1080_4807"));
    }

    #[tokio::test]
    async fn compact_hoists_segment_template_to_adaptation_set() {
        let asset: Asset = serde_json::from_value(serde_json::json!({
            "tracks": [
                { "type": "video", "id": "v0", "source": fixture("video_avc_1080.mp4"),
                  "fourcc": "avc1", "timescale": 90000, "width": 1920, "height": 1080 },
                { "type": "video", "id": "v1", "source": fixture("video_avc_1080.mp4"),
                  "fourcc": "avc1", "timescale": 90000, "width": 1920, "height": 1080 }
            ]
        }))
        .unwrap();

        let verbose = generate_mpd(&asset, Path::new("."), false).await.unwrap();
        let compact = generate_mpd(&asset, Path::new("."), true).await.unwrap();

        // Verbose: each Representation carries its own SegmentTemplate.
        assert_eq!(verbose.matches("<SegmentTemplate").count(), 2);
        // Compact: one SegmentTemplate hoisted to the single AdaptationSet, none per rep.
        assert_eq!(compact.matches("<SegmentTemplate").count(), 1);
        // The template precedes the Representations in document order.
        let st = compact.find("<SegmentTemplate").unwrap();
        let rep = compact.find("<Representation").unwrap();
        assert!(st < rep, "SegmentTemplate must precede Representation");
        // Still a valid live-profile template.
        assert!(compact.contains("$RepresentationID$/$Time$.m4s"));
    }
}
