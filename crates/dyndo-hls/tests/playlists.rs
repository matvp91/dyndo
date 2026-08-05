use std::path::PathBuf;

use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::segment::SegmentOptions;
use dyndo_hls::options::HlsOptions;
use opendal::Operator;
use opendal::services::Memory;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");

#[tokio::test]
async fn generate_master_playlist_emits_video_variant_and_audio_rendition() {
    let (op, asset) = asset().await;

    let playlist = dyndo_hls::builder::generate_master_playlist(
        &op,
        &asset,
        &SegmentOptions::default(),
        &HlsOptions::default(),
    )
    .await
    .unwrap()
    .to_string();

    for expected in [
        "#EXT-X-INDEPENDENT-SEGMENTS",
        "TYPE=AUDIO",
        "GROUP-ID=\"audio\"",
        "LANGUAGE=\"nld\"",
        "NAME=\"Dutch\"",
        "URI=\"audio-nld.m3u8\"",
        "RESOLUTION=1920x1080",
        "FRAME-RATE=25",
        "AUDIO=\"audio\"",
        "CLOSED-CAPTIONS=NONE",
        "video-main.m3u8",
    ] {
        assert!(
            playlist.contains(expected),
            "missing {expected:?} in {playlist}"
        );
    }
}

#[tokio::test]
async fn generate_media_playlist_emits_vod_timing_and_relative_uris() {
    let (op, asset) = asset().await;
    let descriptor = asset.track("video-main").unwrap();

    let playlist = dyndo_hls::builder::generate_media_playlist(
        &op,
        &asset,
        descriptor,
        &SegmentOptions::default(),
        &HlsOptions::default(),
    )
    .await
    .unwrap();
    let playlist = dyndo_hls::builder::serialize_media_playlist(&playlist);

    for expected in [
        "#EXT-X-PLAYLIST-TYPE:VOD",
        "#EXT-X-TARGETDURATION:",
        "#EXT-X-MAP:URI=\"video-main/init.mp4\"",
        "#EXTINF:1.920,",
        "video-main/0.m4s",
        "#EXT-X-ENDLIST",
    ] {
        assert!(
            playlist.contains(expected),
            "missing {expected:?} in {playlist}"
        );
    }
}

#[tokio::test]
async fn generate_media_playlist_applies_requested_minimum_segment_length() {
    let (op, asset) = asset().await;
    let descriptor = asset.track("video-main").unwrap();

    let segment_options = SegmentOptions {
        min_segment_length_ms: 10_000,
    };
    let playlist = dyndo_hls::builder::generate_media_playlist(
        &op,
        &asset,
        descriptor,
        &segment_options,
        &HlsOptions::default(),
    )
    .await
    .unwrap();
    let playlist = dyndo_hls::builder::serialize_media_playlist(&playlist);

    assert!(playlist.contains("#EXT-X-TARGETDURATION:12") && playlist.contains("#EXTINF:11.520,"));
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
