use std::ops::Range;

use dyndo_core::cmaf_track_kind::CmafTrackKind;
use dyndo_core::reader::Reader;
use dyndo_core::source_track::SourceTrack;
use opendal::{Operator, services::Memory};
use relative_path::RelativePath;

const VIDEO_FIXTURE: &[u8] = include_bytes!("fixtures/three-frame-black-h264.mp4");

fn memory_operator() -> Operator {
    Operator::new(Memory::default()).unwrap()
}

fn video_fixture_range(range: Range<u64>) -> &'static [u8] {
    let start = usize::try_from(range.start).unwrap();
    let end = usize::try_from(range.end).unwrap();
    &VIDEO_FIXTURE[start..end]
}

#[tokio::test]
async fn video_probe_and_read_returns_the_indexed_portions_of_a_fragmented_mp4() {
    let operator = memory_operator();
    let path = RelativePath::new("video.mp4");
    operator.write(path.as_str(), VIDEO_FIXTURE).await.unwrap();

    let track = SourceTrack::probe(&operator, path, None).await.unwrap();
    let track = track.cmaf().unwrap();
    let reader = Reader::new(&operator);
    let initialization = reader.read_initialization(track).await.unwrap();
    let media = reader
        .read_range(track, track.segments()[0].byte_range())
        .await
        .unwrap();

    assert!(matches!(
        track.kind(),
        CmafTrackKind::Video(kind) if (kind.width, kind.height, kind.frame_rate.as_str()) == (16, 16, "4/1")
    ));
    assert_eq!(track.id(), "76666358-6bba-5805-9a37-a18e055e9ea8");
    assert_eq!(track.codec().rfc6381(), "avc1.42c00a");
    assert_eq!(track.segments().len(), 1);
    assert_eq!(
        initialization.as_ref(),
        video_fixture_range(track.init_segment().byte_range())
    );
    assert_eq!(
        media.as_ref(),
        video_fixture_range(track.segments()[0].byte_range())
    );
}
