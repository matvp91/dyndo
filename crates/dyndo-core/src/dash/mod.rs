mod build;
mod timeline;

pub(crate) use build::build_mpd;

use serde::Serialize;

use crate::cmaf::read_header;
use crate::error::{Error, Result};
use crate::model::Asset;
use crate::storage::LocalFile;

/// Read every track's source in the CWD, then build + serialize a static DASH MPD,
/// pretty-printed with two-space indentation.
pub async fn generate_mpd(asset: &Asset) -> Result<String> {
    let mut headers = Vec::with_capacity(asset.tracks.len());
    for track in &asset.tracks {
        let source = LocalFile::new(track.source());
        let header = read_header(&source, track.source()).await?;
        headers.push((track.id().to_string(), header));
    }

    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let mut serializer = quick_xml::se::Serializer::new(&mut xml);
    serializer.indent(' ', 2);
    build_mpd(&headers)
        .serialize(serializer)
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

        let xml = generate_mpd(&asset).await.unwrap();
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
}
