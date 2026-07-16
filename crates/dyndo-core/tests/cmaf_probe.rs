//! End-to-end checks that the CMAF parser projects the committed fixtures into
//! the expected `CmafHeader` / `CmafMetadata`. Acts as the regression guard for the
//! streaming rewrite.

use dyndo_core::cmaf::{self, CmafMetadata};
use opendal::Operator;
use opendal::services::Fs;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures");

fn fixtures_op() -> Operator {
    Operator::new(Fs::default().root(FIXTURES)).unwrap()
}

#[tokio::test]
async fn parses_video_avc_fixture() {
    let op = fixtures_op();
    let (h, m) = cmaf::probe(&op, "video_avc_1080.mp4").await.unwrap();

    assert_eq!(h.timescale, 90_000);
    assert_eq!(h.duration, 123_328_800);
    assert_eq!(h.bandwidth, 4_807_228);
    assert_eq!(h.earliest_presentation_time, 0);
    assert_eq!(h.init_segment.offset, 0);
    assert_eq!(h.init_segment.size, 766);
    assert_eq!(h.segments.len(), 715);
    let first = h.segments.first().unwrap();
    assert_eq!(first.offset, 9_386);
    assert_eq!(first.size, 1_495_550);
    assert_eq!(first.duration, 172_800);

    let CmafMetadata::Video(v) = m else {
        panic!("expected a video track");
    };
    assert_eq!(v.codec.fourcc().to_string(), "avc1");
    assert_eq!(v.codec.rfc6381(), "avc1.640028");
    assert_eq!(v.width, 1_920);
    assert_eq!(v.height, 1_080);
    assert_eq!(v.frame_rate, (25, 1));
}

#[tokio::test]
async fn parses_video_hvc1_fixture() {
    let op = fixtures_op();
    let (h, m) = cmaf::probe(&op, "video_hvc1_240.mp4").await.unwrap();

    assert_eq!(h.timescale, 12_800);
    assert_eq!(h.duration, 51_200);
    assert_eq!(h.bandwidth, 57_686);
    assert_eq!(h.earliest_presentation_time, 0);
    assert_eq!(h.init_segment.offset, 0);
    assert_eq!(h.init_segment.size, 3_200);
    assert_eq!(h.segments.len(), 4);
    let first = h.segments.first().unwrap();
    assert_eq!(first.offset, 3_288);
    assert_eq!(first.size, 7_123);
    assert_eq!(first.duration, 12_800);

    let CmafMetadata::Video(v) = m else {
        panic!("expected a video track");
    };
    assert_eq!(v.codec.fourcc().to_string(), "hvc1");
    assert_eq!(v.codec.rfc6381(), "hvc1.1.6.L60.90");
    assert_eq!(v.width, 320);
    assert_eq!(v.height, 240);
    assert_eq!(v.frame_rate, (25, 1));
}

#[tokio::test]
async fn parses_video_hev1_fixture() {
    let op = fixtures_op();
    let (h, m) = cmaf::probe(&op, "video_hev1_240.mp4").await.unwrap();

    assert_eq!(h.timescale, 12_800);
    assert_eq!(h.init_segment.size, 3_200);
    assert_eq!(h.segments.len(), 4);

    let CmafMetadata::Video(v) = m else {
        panic!("expected a video track");
    };
    // Same stream as the hvc1 fixture, tagged hev1: only the fourcc/prefix differ.
    assert_eq!(v.codec.fourcc().to_string(), "hev1");
    assert_eq!(v.codec.rfc6381(), "hev1.1.6.L60.90");
    assert_eq!(v.width, 320);
    assert_eq!(v.height, 240);
}

#[tokio::test]
async fn parses_audio_aac_fixture() {
    let op = fixtures_op();
    let (h, m) = cmaf::probe(&op, "audio_aac_nl_2.mp4").await.unwrap();

    assert_eq!(h.timescale, 48_000);
    assert_eq!(h.duration, 65_775_616);
    assert_eq!(h.bandwidth, 196_918);
    assert_eq!(h.init_segment.size, 662);
    assert_eq!(h.segments.len(), 715);
    let first = h.segments.first().unwrap();
    assert_eq!(first.offset, 9_282);
    assert_eq!(first.size, 48_530);
    assert_eq!(first.duration, 94_208);

    let CmafMetadata::Audio(a) = m else {
        panic!("expected an audio track");
    };
    assert_eq!(a.codec.fourcc().to_string(), "mp4a");
    assert_eq!(a.codec.rfc6381(), "mp4a.40.2");
    assert_eq!(a.sample_rate, 48_000);
    assert_eq!(a.channels, 2);
    assert_eq!(a.language.as_deref(), Some("nld"));
}

#[tokio::test]
async fn parses_video_av1_fixture() {
    let op = fixtures_op();
    let (h, m) = cmaf::probe(&op, "video_av1_240.mp4").await.unwrap();

    assert_eq!(h.timescale, 12_800);
    assert_eq!(h.duration, 25_600);
    assert_eq!(h.bandwidth, 65_256);
    assert_eq!(h.init_segment.size, 783);
    assert_eq!(h.segments.len(), 2);
    let first = h.segments.first().unwrap();
    assert_eq!(first.offset, 847);
    assert_eq!(first.size, 8_342);
    assert_eq!(first.duration, 12_800);

    let CmafMetadata::Video(v) = m else {
        panic!("expected a video track");
    };
    assert_eq!(v.codec.fourcc().to_string(), "av01");
    assert_eq!(v.codec.rfc6381(), "av01.0.00M.08");
    assert_eq!(v.width, 320);
    assert_eq!(v.height, 240);
    assert_eq!(v.frame_rate, (25, 1));
}

#[tokio::test]
async fn parses_audio_ac3_fixture() {
    let op = fixtures_op();
    let (h, m) = cmaf::probe(&op, "audio_ac3_1.mp4").await.unwrap();

    assert_eq!(h.timescale, 48_000);
    assert_eq!(h.duration, 144_128);
    assert_eq!(h.bandwidth, 193_204);
    assert_eq!(h.init_segment.size, 725);
    assert_eq!(h.segments.len(), 3);
    let first = h.segments.first().unwrap();
    assert_eq!(first.offset, 801);
    assert_eq!(first.size, 24_684);
    assert_eq!(first.duration, 48_896);

    let CmafMetadata::Audio(a) = m else {
        panic!("expected an audio track");
    };
    assert_eq!(a.codec.fourcc().to_string(), "ac-3");
    assert_eq!(a.codec.rfc6381(), "ac-3");
    assert_eq!(a.sample_rate, 48_000);
    assert_eq!(a.channels, 1);
    assert_eq!(a.language.as_deref(), Some("und"));
}

#[tokio::test]
async fn parses_audio_ec3_fixture() {
    let op = fixtures_op();
    let (h, m) = cmaf::probe(&op, "audio_ec3_1.mp4").await.unwrap();

    assert_eq!(h.timescale, 48_000);
    assert_eq!(h.init_segment.size, 727);
    assert_eq!(h.segments.len(), 3);

    let CmafMetadata::Audio(a) = m else {
        panic!("expected an audio track");
    };
    assert_eq!(a.codec.fourcc().to_string(), "ec-3");
    assert_eq!(a.codec.rfc6381(), "ec-3");
    assert_eq!(a.sample_rate, 48_000);
    assert_eq!(a.channels, 1);
    assert_eq!(a.language.as_deref(), Some("und"));
}
