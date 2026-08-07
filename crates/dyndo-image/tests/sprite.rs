//! The crate as a caller sees it: a [`Sprite`] describing the sheet, and a track to
//! cut it from.

use dyndo_core::segment::SegmentOptions;
use dyndo_core::track::Track;
use dyndo_image::ThumbnailError;
use dyndo_image::sprite::Sprite;
use opendal::Operator;
use opendal::services::Memory;
use relative_path::RelativePath;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");

const SPRITE: Sprite = Sprite {
    grid: 5,
    cell_width: 320,
    cadence: 10_000,
};

/// The grid and cadence a caller picks decide the two things a manifest needs: how
/// much presentation one sheet covers, and how big the sheet is.
#[test]
fn a_sprite_describes_the_sheet_it_would_cut() {
    assert_eq!(
        (SPRITE.duration(), SPRITE.size((1920, 1080))),
        (250_000, (1600, 900))
    );
}

#[tokio::test]
async fn generate_refuses_a_track_that_is_not_video() {
    let (op, track) = probe("audio_aac_nl_2.mp4").await;

    let error = SPRITE
        .generate(&op, &track, &SegmentOptions::default(), 0)
        .await
        .unwrap_err();

    assert!(matches!(error, ThumbnailError::NotVideo(_)), "{error}");
}

#[tokio::test]
async fn generate_refuses_a_time_no_sheet_starts_at() {
    let (op, track) = probe("video_avc_1080.mp4").await;

    let error = SPRITE
        .generate(
            &op,
            &track,
            &SegmentOptions::default(),
            SPRITE.cadence.into(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, ThumbnailError::NotFound(_)), "{error}");
}

async fn probe(name: &str) -> (Operator, Track) {
    let op = Operator::new(Memory::default()).unwrap();
    op.write(name, std::fs::read(format!("{FIXTURES}/{name}")).unwrap())
        .await
        .unwrap();
    let track = Track::probe(
        &op,
        RelativePath::new(name),
        None,
        &SegmentOptions::default(),
    )
    .await
    .unwrap();

    (op, track)
}
