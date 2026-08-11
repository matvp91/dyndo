use dyndo_core::asset::Asset;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::ResolvedTrack;
use dyndo_core::track::SourceTrack;
use dyndo_core::track::cmaf::CmafKind;
use dyndo_core::track::timed_text::TimedTextFormat;
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

    let ResolvedTrack::TimedText(vtt) = ResolvedTrack::discover(&operator, path).await.unwrap()
    else {
        panic!("expected a WebVTT source track");
    };
    assert!(matches!(vtt.format(), TimedTextFormat::WebVtt(_)));
    let packaged = vtt.package_wvtt(&options).await.unwrap();

    assert!(matches!(packaged.kind(), CmafKind::Text(_)));
    assert_eq!(packaged.source_path(), None);
    assert_eq!(packaged.codec().rfc6381(), "wvtt");
    assert_eq!(packaged.segments().len(), 4);
    let initialization = packaged
        .read_range(&operator, packaged.init_segment().byte_range())
        .await
        .unwrap();
    let media = packaged
        .read_range(&operator, packaged.segments()[0].byte_range())
        .await
        .unwrap();
    assert_eq!(&initialization[4..8], b"ftyp");
    assert_eq!(&media[4..8], b"styp");
    assert_eq!(
        vtt.served_web_vtt_segment(0, &options)
            .await
            .unwrap()
            .as_deref(),
        Some("WEBVTT\n\n00:00:00.500 --> 00:00:00.750\nFirst\n")
    );
    assert!(
        vtt.served_web_vtt_segment(500, &options)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn asset_rejects_packaged_web_vtt_as_a_source_track() {
    let operator = memory_operator();
    let path = RelativePath::new("subtitles/en.vtt");
    operator.write(path.as_str(), VTT).await.unwrap();
    let ResolvedTrack::TimedText(timed_text) =
        ResolvedTrack::discover(&operator, path).await.unwrap()
    else {
        panic!("expected a WebVTT source track");
    };
    let packaged = timed_text
        .package_wvtt(&SegmentOptions::default())
        .await
        .unwrap();
    let packaged = ResolvedTrack::Cmaf(packaged);
    let mut asset = Asset::read_or_new(&operator, RelativePath::new("asset.json"))
        .await
        .unwrap();

    let error = asset.add_source_track(&packaged).unwrap_err();

    assert_eq!(
        error.to_string(),
        "resolved track is not backed by an asset source"
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

    let error = ResolvedTrack::discover(&operator, path)
        .await
        .err()
        .unwrap();

    assert!(error.to_string().contains("missing WEBVTT signature"));
}

#[tokio::test]
async fn track_type_overrides_the_source_file_extension() {
    let operator = memory_operator();
    let path = RelativePath::new("subtitles/en.vtt");
    operator.write(path.as_str(), VTT).await.unwrap();
    let track: SourceTrack = serde_json::from_str(
        r#"{"id":"video","path":"subtitles/en.vtt","codec":"avc1.42c00a","type":"video","width":16,"height":16,"frame_rate":"4/1"}"#,
    )
    .unwrap();

    let result = track.resolve(&operator, path).await;

    assert!(result.is_err());
}
