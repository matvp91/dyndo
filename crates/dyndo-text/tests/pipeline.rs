//! The crate's three transformations, exercised end to end through its public API.
//!
//! Each is reversible, so most of what matters here is a round trip: a document that
//! survives being divided, packed, unpacked, merged and written again is the whole
//! promise the crate makes.

use dyndo_text::subtitle::Subtitle;
use dyndo_text::{fragmenter, vtt, wvtt};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");

/// Overlapping cues, a gap, and a splice point that falls inside a cue.
fn sample_document() -> String {
    std::fs::read_to_string(format!("{FIXTURES}/text_sample.vtt")).unwrap()
}

fn sample_subtitle() -> Subtitle {
    vtt::parse(&sample_document()).unwrap()
}

#[test]
fn parses_the_sample_document() {
    let subtitle = sample_subtitle();

    assert_eq!(
        subtitle
            .cues
            .iter()
            .map(|cue| (cue.start, cue.end, cue.text.as_str()))
            .collect::<Vec<_>>(),
        [
            (0, 3_000, "Welcome to dyndo"),
            (2_000, 5_000, "Overlapping caption"),
            (10_000, 12_500, "After a gap"),
        ]
    );
}

#[test]
fn a_document_survives_being_written_and_parsed_again() {
    let subtitle = sample_subtitle();

    assert_eq!(vtt::parse(&vtt::write(&subtitle)).unwrap(), subtitle);
}

#[test]
fn dividing_and_merging_returns_the_authored_cues() {
    let subtitle = sample_subtitle();

    for (boundaries, length) in [
        (&[][..], 0),
        (&[][..], 1_000),
        (&[7_400][..], 4_000),
        (&[2_500, 11_000][..], 0),
    ] {
        let fragments = fragmenter::fragment(&subtitle, boundaries, length);

        assert_eq!(
            fragmenter::merge(&fragments),
            subtitle,
            "lost cues at boundaries {boundaries:?} and length {length}"
        );
    }
}

#[test]
fn a_document_survives_the_whole_pipeline() {
    let document = sample_document();
    let subtitle = vtt::parse(&document).unwrap();
    let track = wvtt::pack(&fragmenter::fragment(&subtitle, &[7_400], 4_000)).unwrap();

    // `unpack` reads the fragments out of whatever it is given, so a whole track
    // works here as well as one of its segments does.
    let unpacked = wvtt::unpack(&track, 1_000).unwrap();
    let written = vtt::write(&fragmenter::merge(&unpacked));

    assert_eq!(vtt::parse(&written).unwrap(), subtitle);
}

#[test]
fn every_fragment_of_a_segment_is_unpacked() {
    let subtitle = sample_subtitle();
    let fragments = fragmenter::fragment(&subtitle, &[], 4_000);
    let track = wvtt::pack(&fragments).unwrap();

    let unpacked = wvtt::unpack(&track, 1_000).unwrap();

    assert_eq!(
        unpacked
            .iter()
            .map(|fragment| (fragment.start, fragment.end))
            .collect::<Vec<_>>(),
        fragments
            .iter()
            .map(|fragment| (fragment.start, fragment.end))
            .collect::<Vec<_>>()
    );
}

/// The one thing the round trip cannot preserve, and why it does not matter: two cues
/// carrying the same text with no gap between them are indistinguishable once the
/// timeline is cut into samples, so they come back as one.
#[test]
fn adjacent_cues_carrying_the_same_text_come_back_as_one() {
    let subtitle =
        vtt::parse("WEBVTT\n\n00:00.000 --> 00:01.000\nsame\n\n00:01.000 --> 00:02.000\nsame\n")
            .unwrap();
    let track = wvtt::pack(&fragmenter::fragment(&subtitle, &[], 0)).unwrap();

    let merged = fragmenter::merge(&wvtt::unpack(&track, 1_000).unwrap());

    assert_eq!(
        merged
            .cues
            .iter()
            .map(|cue| (cue.start, cue.end, cue.text.as_str()))
            .collect::<Vec<_>>(),
        [(0, 2_000, "same")]
    );
}

#[test]
fn unpacking_reads_media_time_in_the_tracks_timescale() {
    let subtitle = vtt::parse("WEBVTT\n\n00:00.000 --> 00:02.000\nHello\n").unwrap();
    let track = wvtt::pack(&fragmenter::fragment(&subtitle, &[], 0)).unwrap();

    // The packer counts in milliseconds, so reading it at 500 Hz doubles every time.
    let merged = fragmenter::merge(&wvtt::unpack(&track, 500).unwrap());

    assert_eq!((merged.cues[0].start, merged.cues[0].end), (0, 4_000));
}

#[test]
fn a_document_without_cues_cannot_be_packed() {
    let subtitle = vtt::parse("WEBVTT\n").unwrap();

    let error = wvtt::pack(&fragmenter::fragment(&subtitle, &[], 0)).unwrap_err();

    assert!(matches!(error, wvtt::PackError::Empty));
}

#[test]
fn a_truncated_segment_cannot_be_unpacked() {
    let subtitle = sample_subtitle();
    let mut track = wvtt::pack(&fragmenter::fragment(&subtitle, &[], 0)).unwrap();
    track.truncate(track.len() - 4);

    let error = wvtt::unpack(&track, 1_000).unwrap_err();

    assert!(matches!(error, wvtt::UnpackError::UnpairedFragment));
}

#[test]
fn a_malformed_document_is_rejected() {
    assert!(matches!(
        vtt::parse("00:00.000 --> 00:01.000\nx"),
        Err(vtt::ParseError::MissingSignature)
    ));
}
