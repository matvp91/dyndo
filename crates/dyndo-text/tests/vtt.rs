use dyndo_text::vtt;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");

#[test]
fn parses_the_sample_document() {
    let document = std::fs::read_to_string(format!("{FIXTURES}/text_sample.vtt")).unwrap();

    let subtitle = vtt::parse(&document).unwrap();

    assert_eq!(
        subtitle
            .cues
            .iter()
            .map(|cue| (cue.start_ms, cue.end_ms, cue.text.as_str()))
            .collect::<Vec<_>>(),
        [
            (0, 3_000, "Welcome to dyndo"),
            (2_000, 5_000, "Overlapping caption"),
            (10_000, 12_500, "After a gap"),
        ]
    );
}
