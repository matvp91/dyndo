use dyndo_core::packaging::wvtt::{WvttPackager, WvttSample, WvttUnpackager};
use dyndo_core::packaging::{MediaSegment, Sample};
use dyndo_core::text::{Cue, Subtitle};

#[test]
fn vtt_wvtt_roundtrip_preserves_overlapping_cues() {
    let source =
        "WEBVTT\n\n00:00:00.500 --> 00:00:01.500\nFirst\n\n00:00:01.000 --> 00:00:02.500\nSecond\n";

    let packaged = Subtitle::from_vtt_text(source)
        .unwrap()
        .to_wvtt(1_000, &[750])
        .unwrap();
    let roundtripped = Subtitle::from_wvtt(&packaged).unwrap();

    assert_eq!(
        roundtripped.cues,
        vec![
            Cue {
                start: 500,
                end: 1_500,
                text: "First".into()
            },
            Cue {
                start: 1_000,
                end: 2_500,
                text: "Second".into()
            },
        ]
    );
}

#[test]
fn wvtt_packager_and_unpackager_preserve_media_timeline() {
    let segments = vec![
        MediaSegment::new(
            0,
            vec![Sample::new(500, WvttSample::new(vec!["one".into()]))],
        ),
        MediaSegment::new(
            500,
            vec![
                Sample::new(250, WvttSample::new(Vec::new())),
                Sample::new(250, WvttSample::new(vec!["two".into(), "three".into()])),
            ],
        ),
    ];

    let bytes = WvttPackager::new(1_000)
        .with_track_id(7)
        .package(&segments)
        .unwrap();
    let unpackaged = WvttUnpackager::new().unpackage(&bytes).unwrap();

    assert_eq!(unpackaged.timescale(), 1_000);
    assert_eq!(unpackaged.segments(), segments);
}
