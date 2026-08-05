use std::path::PathBuf;

use dyndo_core::asset_descriptor::AssetDescriptor;
use opendal::Operator;
use opendal::services::Memory;
use serde::Serialize;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");

#[tokio::test]
async fn generate_mpd_emits_complete_vod_manifest() {
    let (op, asset) = asset().await;

    let mpd = dyndo_dash::builder::generate_mpd(&op, &asset)
        .await
        .unwrap();
    let mut xml = String::new();
    let mut serializer = quick_xml::se::Serializer::new(&mut xml);
    serializer.indent(' ', 2);
    mpd.serialize(serializer).unwrap();

    for expected in [
        "type=\"static\"",
        "mediaPresentationDuration=\"PT",
        "minBufferTime=\"PT",
        "contentType=\"video\"",
        "contentType=\"audio\"",
        "segmentAlignment=\"true\"",
        "startWithSAP=\"1\"",
        "id=\"video-main\"",
        "codecs=\"avc1.640028\"",
        "width=\"1920\"",
        "height=\"1080\"",
        "frameRate=\"25/1\"",
        "id=\"audio-nld\"",
        "codecs=\"mp4a.40.2\"",
        "audioSamplingRate=\"48000\"",
        "lang=\"nld\"",
        "initialization=\"$RepresentationID$/init.mp4\"",
        "media=\"$RepresentationID$/$Time$.m4s\"",
        "<SegmentTimeline>",
    ] {
        assert!(xml.contains(expected), "missing {expected:?} in {xml}");
    }
}

async fn asset() -> (Operator, AssetDescriptor) {
    let op = Operator::new(Memory::default()).unwrap();
    stage(&op, "video_avc_1080.mp4").await;
    stage(&op, "audio_aac_nl_2.mp4").await;
    op.write(
        "asset.json",
        r#"{
          "tracks": [
            {
              "id": "video-main",
              "path": "video_avc_1080.mp4",
              "codec": "avc1.640028",
              "type": "video",
              "width": 1920,
              "height": 1080,
              "frame_rate": "25/1"
            },
            {
              "id": "audio-nld",
              "path": "audio_aac_nl_2.mp4",
              "codec": "mp4a.40.2",
              "type": "audio",
              "sample_rate": 48000,
              "channels": 2,
              "language": "nld"
            }
          ]
        }"#,
    )
    .await
    .unwrap();
    let asset = AssetDescriptor::read(&op, "asset.json").await.unwrap();
    (op, asset)
}

async fn stage(op: &Operator, name: &str) {
    let bytes = std::fs::read(PathBuf::from(FIXTURES).join(name)).unwrap();
    op.write(name, bytes).await.unwrap();
}
