use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::Track;
use mp4_atom::{Any, DecodeMaybe, Encode, Sidx};
use opendal::{Operator, services::Memory};
use relative_path::RelativePath;

const VIDEO_FIXTURE: &[u8] = include_bytes!("fixtures/three-frame-black-h264.mp4");

fn memory_operator() -> Operator {
    Operator::new(Memory::default()).unwrap()
}

fn rewrite_sidx(transform: impl FnOnce(&mut Sidx)) -> Vec<u8> {
    let mut input = VIDEO_FIXTURE;
    let mut output = Vec::new();
    let mut transform = Some(transform);

    while let Some(mut atom) = Any::decode_maybe(&mut input).unwrap() {
        if let Any::Sidx(sidx) = &mut atom {
            transform.take().unwrap()(sidx);
        }
        atom.encode(&mut output).unwrap();
    }

    output
}

#[tokio::test]
async fn probe_rejects_a_sidx_with_zero_timescale() {
    let operator = memory_operator();
    operator
        .write("video.mp4", rewrite_sidx(|sidx| sidx.timescale = 0))
        .await
        .unwrap();

    let error = Track::probe(
        &operator,
        RelativePath::new("video.mp4"),
        None,
        &SegmentOptions::default(),
    )
    .await
    .err()
    .unwrap();

    assert_eq!(
        error.to_string(),
        "invalid track container: sidx timescale is zero"
    );
}

#[tokio::test]
async fn probe_rejects_a_sidx_reference_with_zero_duration() {
    let operator = memory_operator();
    operator
        .write(
            "video.mp4",
            rewrite_sidx(|sidx| sidx.references[0].subsegment_duration = 0),
        )
        .await
        .unwrap();

    let error = Track::probe(
        &operator,
        RelativePath::new("video.mp4"),
        None,
        &SegmentOptions::default(),
    )
    .await
    .err()
    .unwrap();

    assert_eq!(
        error.to_string(),
        "invalid track container: sidx reference duration is zero"
    );
}

#[tokio::test]
async fn probe_rejects_a_sidx_reference_without_a_random_access_point() {
    let operator = memory_operator();
    operator
        .write(
            "video.mp4",
            rewrite_sidx(|sidx| sidx.references[0].starts_with_sap = false),
        )
        .await
        .unwrap();

    let error = Track::probe(
        &operator,
        RelativePath::new("video.mp4"),
        None,
        &SegmentOptions::default(),
    )
    .await
    .err()
    .unwrap();

    assert_eq!(error.to_string(), "invalid sidx reference");
}
