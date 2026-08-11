use dyndo_core::cmaf_track_kind::CmafTrackKind;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::text::{Cue, Subtitle};
use dyndo_core::track::Track;
use opendal::{Operator, services::Memory};
use relative_path::RelativePath;

const VTT: &str =
    "WEBVTT\n\n00:00:00.500 --> 00:00:01.500\nFirst\n\n00:00:01.000 --> 00:00:02.500\nSecond\n";

fn memory_operator() -> Operator {
    Operator::new(Memory::default()).unwrap()
}

#[tokio::test]
async fn vtt_track_packages_cmaf_on_demand_and_serves_vtt_directly() {
    let operator = memory_operator();
    let path = RelativePath::new("subtitles/en.vtt");
    let options = SegmentOptions {
        text_length: 1_000,
        boundaries: vec![750],
        ..SegmentOptions::default()
    };
    operator.write(path.as_str(), VTT).await.unwrap();

    let track = Track::probe(&operator, path, None).await.unwrap();
    let vtt = track.vtt().unwrap();
    let packaged = vtt.package(&options).await.unwrap();
    let end = packaged.cmaf().segments().last().unwrap().byte_range().end;
    let subtitle = Subtitle::from_wvtt(&packaged.read(0..end).unwrap()).unwrap();

    assert!(matches!(packaged.cmaf().kind(), CmafTrackKind::Text(_)));
    assert_eq!(packaged.cmaf().codec().rfc6381(), "wvtt");
    assert_eq!(packaged.cmaf().segments().len(), 4);
    assert_eq!(
        vtt.vtt_segment(0, 750).as_deref(),
        Some("WEBVTT\n\n00:00:00.500 --> 00:00:00.750\nFirst\n")
    );
    assert_eq!(
        vtt.vtt_segment(0, 1_500).as_deref(),
        Some(
            "WEBVTT\n\n00:00:00.500 --> 00:00:01.500\nFirst\n\n00:00:01.000 --> 00:00:01.500\nSecond\n"
        )
    );
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

#[tokio::test]
async fn vtt_track_reports_an_invalid_subtitle_document() {
    let operator = memory_operator();
    let path = RelativePath::new("subtitles/invalid.vtt");
    operator
        .write(path.as_str(), "this is not a WebVTT document")
        .await
        .unwrap();

    let error = Track::probe(&operator, path, None).await.err().unwrap();

    assert!(error.to_string().contains("missing WEBVTT signature"));
}
