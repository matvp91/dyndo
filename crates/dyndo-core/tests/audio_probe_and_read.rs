use std::ops::Range;

use dyndo_core::reader::Reader;
use dyndo_core::track::SourceTrack;
use dyndo_core::track::cmaf::kind::CmafTrackKind;
use opendal::{Operator, services::Memory};
use relative_path::RelativePath;

const AUDIO_FIXTURE: &[u8] = include_bytes!("fixtures/one-second-silence-aac.mp4");

fn memory_operator() -> Operator {
    Operator::new(Memory::default()).unwrap()
}

fn audio_fixture_range(range: Range<u64>) -> &'static [u8] {
    let start = usize::try_from(range.start).unwrap();
    let end = usize::try_from(range.end).unwrap();
    &AUDIO_FIXTURE[start..end]
}

#[tokio::test]
async fn aac_probe_and_read_remains_a_small_secondary_media_smoke_test() {
    let operator = memory_operator();
    let path = RelativePath::new("audio.mp4");
    operator.write(path.as_str(), AUDIO_FIXTURE).await.unwrap();

    let track = SourceTrack::probe(&operator, path, None).await.unwrap();
    let track = track.cmaf().unwrap();
    let reader = Reader::new(&operator);
    let media = reader
        .read_range(track, track.segments()[0].byte_range())
        .await
        .unwrap();

    assert!(matches!(
        track.kind(),
        CmafTrackKind::Audio(kind) if (kind.sample_rate, kind.channels) == (8_000, 1)
    ));
    assert_eq!(track.codec().rfc6381(), "mp4a.40.2");
    assert_eq!(
        media.as_ref(),
        audio_fixture_range(track.segments()[0].byte_range())
    );
}
