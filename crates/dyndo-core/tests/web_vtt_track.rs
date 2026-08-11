use dyndo_core::asset::track::TrackDescriptor;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::SourceTrack;
use dyndo_core::track::cmaf::kind::CmafTrackKind;
use dyndo_core::track::timed_text::TimedTextTrack;
use opendal::{Operator, services::Memory};
use relative_path::RelativePath;

const VTT: &str =
    "WEBVTT\n\n00:00:00.500 --> 00:00:01.500\nFirst\n\n00:00:01.000 --> 00:00:02.500\nSecond\n";

fn memory_operator() -> Operator {
    Operator::new(Memory::default()).unwrap()
}

#[tokio::test]
async fn web_vtt_track_packages_cmaf_on_demand_and_serves_vtt_directly() {
    let operator = memory_operator();
    let path = RelativePath::new("subtitles/en.vtt");
    let options = SegmentOptions {
        text_length: 1_000,
        boundaries: vec![750],
        ..SegmentOptions::default()
    };
    operator.write(path.as_str(), VTT).await.unwrap();

    let SourceTrack::TimedText(TimedTextTrack::WebVtt(vtt)) =
        SourceTrack::probe(&operator, path, None).await.unwrap()
    else {
        panic!("expected a WebVTT source track");
    };
    let packaged = vtt.package(&options).await.unwrap();

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
}

#[tokio::test]
async fn web_vtt_track_reports_an_invalid_subtitle_document() {
    let operator = memory_operator();
    let path = RelativePath::new("subtitles/invalid.vtt");
    operator
        .write(path.as_str(), "this is not a WebVTT document")
        .await
        .unwrap();

    let error = SourceTrack::probe(&operator, path, None)
        .await
        .err()
        .unwrap();

    assert!(error.to_string().contains("missing WEBVTT signature"));
}

#[tokio::test]
async fn descriptor_type_overrides_the_source_file_extension() {
    let operator = memory_operator();
    let path = RelativePath::new("subtitles/en.vtt");
    operator.write(path.as_str(), VTT).await.unwrap();
    let descriptor: TrackDescriptor = serde_json::from_str(
        r#"{"id":"video","path":"subtitles/en.vtt","codec":"avc1.42c00a","type":"video","width":16,"height":16,"frame_rate":"4/1"}"#,
    )
    .unwrap();

    let result = SourceTrack::probe(&operator, path, Some(&descriptor)).await;

    assert!(result.is_err());
}
