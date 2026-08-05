use std::path::PathBuf;

use dyndo_core::asset_descriptor::TrackKind;
use dyndo_core::track::{Track, TrackError};
use dyndo_core::track_probe::TrackProbeError;
use opendal::Operator;
use opendal::services::Memory;
use relative_path::RelativePath;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");

#[tokio::test]
async fn probe_reads_avc_video_metadata() {
    let (_, track, _) = probe_fixture("video_avc_1080.mp4").await;

    assert_video(&track, "avc1.640028", 1920, 1080, "25/1");
}

#[tokio::test]
async fn probe_reads_aac_audio_metadata() {
    let (_, track, _) = probe_fixture("audio_aac_nl_2.mp4").await;

    let TrackKind::Audio(audio) = track.kind() else {
        panic!("expected audio track");
    };
    assert_eq!(
        (
            track.codec(),
            audio.sample_rate,
            audio.channels,
            audio.language.as_str()
        ),
        ("mp4a.40.2", 48_000, 2, "nld")
    );
}

#[tokio::test]
async fn probe_reads_fragmented_wvtt_metadata() {
    let (_, track, _) = probe_fixture("text_wvtt_eng.mp4").await;

    let TrackKind::Text(text) = track.kind() else {
        panic!("expected text track");
    };
    assert_eq!((track.codec(), text.language.as_str()), ("wvtt", "eng"));
}

#[tokio::test]
async fn probe_exposes_valid_initialization_and_declared_segment_ranges() {
    let (op, track, source) = probe_fixture("video_avc_1080.mp4").await;
    let initialization_range = track.initialization_range();
    let segment = track.segments(&[], 0).into_iter().next().unwrap();

    let initialization = track.read_initialization(&op).await.unwrap();
    let initialization_range = usize::try_from(initialization_range.start).unwrap()
        ..usize::try_from(initialization_range.end).unwrap();

    assert_eq!(
        (initialization.as_ref(), segment.byte_range().start),
        (&source[initialization_range], 9_386)
    );
}

#[tokio::test]
async fn probe_rejects_non_sap_codec_fixtures() {
    for name in [
        "video_av1_240.mp4",
        "video_hvc1_240.mp4",
        "video_hev1_240.mp4",
        "audio_ac3_1.mp4",
        "audio_ec3_1.mp4",
    ] {
        let error = probe_fixture_error(name).await;
        assert!(
            matches!(
                error,
                TrackError::Probe(TrackProbeError::BoxReader(
                    dyndo_core::box_reader::BoxReaderError::InvalidSidxReference
                ))
            ),
            "unexpected error for {name}: {error}"
        );
    }
}

#[tokio::test]
async fn probe_generates_deterministic_content_prefixed_id() {
    let (op, first, _) = probe_fixture("video_avc_1080.mp4").await;
    let second = Track::probe(&op, RelativePath::new("video_avc_1080.mp4"), None)
        .await
        .unwrap();

    assert!(first.id() == second.id() && first.id().starts_with("video_"));
}

#[tokio::test]
async fn probe_raw_vtt_uses_placeholder_cmaf_track() {
    let (_, track, _) = probe_fixture("text_sample.vtt").await;

    assert_eq!(
        (
            track.codec(),
            track.timescale(),
            track.initialization_range(),
            track.segments(&[], 0).len(),
        ),
        ("wvtt", 1000, 0..0, 0)
    );
}

#[tokio::test]
async fn probe_rejects_unknown_file_extension() {
    let op = Operator::new(Memory::default()).unwrap();

    let error = match Track::probe(&op, RelativePath::new("track.bin"), None).await {
        Ok(_) => panic!("unknown extension unexpectedly probed"),
        Err(error) => error,
    };

    assert!(matches!(
        error,
        TrackError::Probe(TrackProbeError::UnsupportedFormat)
    ));
}

async fn probe_fixture(name: &str) -> (Operator, Track, Vec<u8>) {
    let source = std::fs::read(fixture(name)).unwrap();
    let op = Operator::new(Memory::default()).unwrap();
    op.write(name, source.clone()).await.unwrap();
    let track = Track::probe(&op, RelativePath::new(name), None)
        .await
        .unwrap();
    (op, track, source)
}

async fn probe_fixture_error(name: &str) -> TrackError {
    let source = std::fs::read(fixture(name)).unwrap();
    let op = Operator::new(Memory::default()).unwrap();
    op.write(name, source).await.unwrap();
    match Track::probe(&op, RelativePath::new(name), None).await {
        Ok(_) => panic!("{name} unexpectedly probed"),
        Err(error) => error,
    }
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(FIXTURES).join(name)
}

fn assert_video(track: &Track, codec: &str, width: u32, height: u32, frame_rate: &str) {
    let TrackKind::Video(video) = track.kind() else {
        panic!("expected video track");
    };
    assert_eq!(
        (
            track.codec(),
            video.width,
            video.height,
            video.frame_rate.as_str()
        ),
        (codec, width, height, frame_rate)
    );
}
