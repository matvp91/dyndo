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
    let (track, _) = probe_fixture("video_avc_1080.mp4").await;

    assert_video(&track, "avc1.640028", 1920, 1080, "25/1");
}

#[tokio::test]
async fn probe_reads_aac_audio_metadata() {
    let (track, _) = probe_fixture("audio_aac_nl_2.mp4").await;

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
    let (track, _) = probe_fixture("text_wvtt_eng.mp4").await;

    let TrackKind::Text(text) = track.kind() else {
        panic!("expected text track");
    };
    assert_eq!((track.codec(), text.language.as_str()), ("wvtt", "eng"));
}

#[tokio::test]
async fn probe_exposes_valid_initialization_and_declared_segment_ranges() {
    let (track, source) = probe_fixture("video_avc_1080.mp4").await;
    let initialization_range = track.initialization_range();
    let segment = track.segments(&[], 0).into_iter().next().unwrap();

    let initialization = track.read_initialization().await.unwrap();
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
    let (first, _) = probe_fixture("video_avc_1080.mp4").await;
    let (second, _) = probe_fixture("video_avc_1080.mp4").await;

    assert!(first.id() == second.id() && first.id().starts_with("video_"));
}

#[tokio::test]
async fn probe_packages_a_vtt_document_as_a_wvtt_track() {
    let track = probe_fixture_with("text_sample.vtt", &[], 4_000).await;

    let TrackKind::Text(_) = track.kind() else {
        panic!("expected text track");
    };
    assert_eq!(
        (track.codec(), track.timescale(), track.duration_ms()),
        ("wvtt", 1_000, 12_500)
    );

    // The fixture's cues end at 12.5s, so a 4s grid cuts at 4s, 8s and 12s.
    let segments = track.segments(&[], 4_000);
    assert_eq!(
        segments
            .iter()
            .map(|segment| segment.duration())
            .collect::<Vec<_>>(),
        vec![4_000, 4_000, 4_000, 500]
    );

    let initialization = track.read_initialization().await.unwrap();
    assert!(
        initialization.windows(4).any(|kind| kind == b"wvtt"),
        "initialization segment declares no wvtt sample entry"
    );
    let segment = track.read_range(segments[0].byte_range()).await.unwrap();
    assert_eq!(
        u64::try_from(segment.len()).unwrap(),
        segments[0].byte_size()
    );
}

#[tokio::test]
async fn probe_fragments_a_subtitle_at_its_splice_points() {
    // The splice at 2s falls inside the fixture's first cue, so it can only be
    // honoured by packing the track with it.
    let spliced = probe_fixture_with("text_sample.vtt", &[2_000], 0).await;
    let unspliced = probe_fixture_with("text_sample.vtt", &[], 0).await;

    let durations = |track: &Track| {
        track
            .segments(&[], 0)
            .iter()
            .map(|segment| segment.duration())
            .collect::<Vec<_>>()
    };
    assert_eq!(durations(&spliced), vec![2_000, 10_500]);
    assert_eq!(durations(&unspliced), vec![12_500]);
}

#[tokio::test]
async fn probe_rejects_unknown_file_extension() {
    let op = Operator::new(Memory::default()).unwrap();

    let error = match Track::probe(&op, RelativePath::new("track.bin"), None, &[], 0).await {
        Ok(_) => panic!("unknown extension unexpectedly read"),
        Err(error) => error,
    };

    assert!(matches!(
        error,
        TrackError::Probe(TrackProbeError::UnsupportedFormat)
    ));
}

async fn probe_fixture_with(
    name: &str,
    boundaries_ms: &[u64],
    min_segment_length_ms: u64,
) -> Track {
    let source = std::fs::read(fixture(name)).unwrap();
    let op = Operator::new(Memory::default()).unwrap();
    op.write(name, source).await.unwrap();
    Track::probe(
        &op,
        RelativePath::new(name),
        None,
        boundaries_ms,
        min_segment_length_ms,
    )
    .await
    .unwrap()
}

async fn probe_fixture(name: &str) -> (Track, Vec<u8>) {
    let source = std::fs::read(fixture(name)).unwrap();
    let op = Operator::new(Memory::default()).unwrap();
    op.write(name, source.clone()).await.unwrap();
    let track = Track::probe(&op, RelativePath::new(name), None, &[], 0)
        .await
        .unwrap();
    (track, source)
}

async fn probe_fixture_error(name: &str) -> TrackError {
    let source = std::fs::read(fixture(name)).unwrap();
    let op = Operator::new(Memory::default()).unwrap();
    op.write(name, source).await.unwrap();
    match Track::probe(&op, RelativePath::new(name), None, &[], 0).await {
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
