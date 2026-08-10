use dyndo_core::reader::Reader;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::Track;
use dyndo_core::track_kind::TrackKind;
use opendal::{Operator, services::Memory};
use relative_path::RelativePath;
use std::ops::Range;

const FIXTURE: &[u8] = include_bytes!("fixtures/one-second-silence-aac.mp4");
const VIDEO_FIXTURE: &[u8] = include_bytes!("fixtures/three-frame-black-h264.mp4");

fn memory_operator() -> Operator {
    Operator::new(Memory::default()).unwrap()
}

fn fixture_range(range: Range<u64>) -> &'static [u8] {
    let start = usize::try_from(range.start).unwrap();
    let end = usize::try_from(range.end).unwrap();
    &FIXTURE[start..end]
}

#[tokio::test]
async fn probe_and_read_returns_the_indexed_portions_of_a_fragmented_mp4() {
    let operator = memory_operator();
    let path = RelativePath::new("video.mp4");
    operator.write(path.as_str(), FIXTURE).await.unwrap();

    let track = Track::probe(&operator, path, None, &SegmentOptions::default())
        .await
        .unwrap();
    let reader = Reader::new(&operator, &track, &SegmentOptions::default());
    let initialization = reader.read_initialization().await.unwrap();
    let media = reader
        .read_range(track.segments()[0].byte_range())
        .await
        .unwrap();

    assert!(matches!(track.kind(), TrackKind::Audio(_)));
    assert_eq!(track.codec().rfc6381(), "mp4a.40.2");
    assert_eq!(track.segments().len(), 1);
    assert_eq!(
        initialization.as_ref(),
        fixture_range(track.init_segment().byte_range())
    );
    assert_eq!(
        media.as_ref(),
        fixture_range(track.segments()[0].byte_range())
    );
}

#[tokio::test]
async fn probe_reports_video_dimensions_frame_rate_and_codec() {
    let operator = memory_operator();
    operator.write("video.mp4", VIDEO_FIXTURE).await.unwrap();

    let track = Track::probe(
        &operator,
        RelativePath::new("video.mp4"),
        None,
        &SegmentOptions::default(),
    )
    .await
    .unwrap();

    assert_eq!(track.codec().rfc6381(), "avc1.42c00a");
    assert_eq!(track.segments().len(), 1);
    assert!(matches!(
        track.kind(),
        TrackKind::Video(kind) if (kind.width, kind.height, kind.frame_rate.as_str()) == (16, 16, "4/1")
    ));
}
