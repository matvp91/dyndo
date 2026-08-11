use dyndo_core::track::Track;
use mp4_atom::{Any, DecodeMaybe, Encode, FourCC, Sidx};
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

fn rewrite_fixture(mut transform: impl FnMut(&mut Any)) -> Vec<u8> {
    let mut input = VIDEO_FIXTURE;
    let mut output = Vec::new();

    while let Some(mut atom) = Any::decode_maybe(&mut input).unwrap() {
        transform(&mut atom);
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

    let error = Track::probe(&operator, RelativePath::new("video.mp4"), None)
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

    let error = Track::probe(&operator, RelativePath::new("video.mp4"), None)
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

    let error = Track::probe(&operator, RelativePath::new("video.mp4"), None)
        .await
        .err()
        .unwrap();

    assert_eq!(error.to_string(), "invalid sidx reference");
}

#[tokio::test]
async fn probe_rejects_a_sidx_with_an_overflowing_segment_time() {
    let operator = memory_operator();
    operator
        .write(
            "video.mp4",
            rewrite_sidx(|sidx| {
                sidx.earliest_presentation_time = u64::MAX;
                sidx.references[0].subsegment_duration = 1;
            }),
        )
        .await
        .unwrap();

    let error = Track::probe(&operator, RelativePath::new("video.mp4"), None)
        .await
        .err()
        .unwrap();

    assert_eq!(error.to_string(), "segment time overflows");
}

#[tokio::test]
async fn probe_rejects_an_unsupported_track_handler() {
    let operator = memory_operator();
    operator
        .write(
            "video.mp4",
            rewrite_fixture(|atom| {
                if let Any::Moov(moov) = atom {
                    moov.trak[0].mdia.hdlr.handler = FourCC::new(b"meta");
                }
            }),
        )
        .await
        .unwrap();

    let error = Track::probe(&operator, RelativePath::new("video.mp4"), None)
        .await
        .err()
        .unwrap();

    assert_eq!(error.to_string(), "unsupported track handler");
}

#[tokio::test]
async fn probe_rejects_a_video_without_sample_duration() {
    let operator = memory_operator();
    operator
        .write(
            "video.mp4",
            rewrite_fixture(|atom| {
                if let Any::Moof(moof) = atom {
                    moof.traf[0].trun[0].entries[0].duration = None;
                }
            }),
        )
        .await
        .unwrap();

    let error = Track::probe(&operator, RelativePath::new("video.mp4"), None)
        .await
        .err()
        .unwrap();

    assert_eq!(error.to_string(), "video track has no sample duration");
}

#[tokio::test]
async fn probe_rejects_a_truncated_container_without_panicking() {
    let operator = memory_operator();
    operator
        .write("video.mp4", &VIDEO_FIXTURE[..VIDEO_FIXTURE.len() / 2])
        .await
        .unwrap();

    let error = Track::probe(&operator, RelativePath::new("video.mp4"), None)
        .await
        .err()
        .unwrap();

    assert!(!error.to_string().is_empty());
}
