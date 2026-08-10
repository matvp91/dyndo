use dyndo_core::reader::Reader;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::text::{Cue, Subtitle};
use dyndo_core::track::Track;
use dyndo_core::track_kind::TrackKind;
use opendal::{Operator, services::Memory};
use relative_path::RelativePath;

const VTT: &str =
    "WEBVTT\n\n00:00:00.500 --> 00:00:01.500\nFirst\n\n00:00:01.000 --> 00:00:02.500\nSecond\n";

fn memory_operator() -> Operator {
    Operator::new(Memory::default()).unwrap()
}

#[tokio::test]
async fn vtt_reader_layer_probes_and_serves_packaged_subtitles() {
    let operator = memory_operator();
    let path = RelativePath::new("subtitles/en.vtt");
    let options = SegmentOptions {
        text_length: 1_000,
        boundaries: vec![750],
        ..SegmentOptions::default()
    };
    operator.write(path.as_str(), VTT).await.unwrap();

    let track = Track::probe(&operator, path, None, &options).await.unwrap();
    let reader = Reader::new(&operator, &track, &options);
    let packaged = reader
        .read_range(0..track.segments().last().unwrap().byte_range().end)
        .await
        .unwrap();
    let subtitle = Subtitle::from_wvtt(&packaged).unwrap();

    assert!(matches!(track.kind(), TrackKind::Text(_)));
    assert_eq!(track.codec().rfc6381(), "wvtt");
    assert_eq!(track.segments().len(), 4);
    assert_eq!(
        subtitle.cues,
        vec![
            Cue {
                start: 500,
                end: 1_500,
                text: "First".into(),
            },
            Cue {
                start: 1_000,
                end: 2_500,
                text: "Second".into(),
            },
        ]
    );
}
