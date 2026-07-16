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
