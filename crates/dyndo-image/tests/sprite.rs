//! The crate as a caller sees it: the sprite asked for, and a track to cut it from.

use dyndo_core::segment::SegmentOptions;
use dyndo_core::track::Track;
use dyndo_image::sprite::{Sprite, SpriteError};
use opendal::Operator;
use opendal::services::Memory;
use relative_path::RelativePath;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");
const TILE_SIZE: u32 = 5;
const STEP: u32 = 10_000;

#[tokio::test]
async fn generate_refuses_a_track_that_is_not_video() {
    let (op, track) = probe("audio_aac_nl_2.mp4").await;

    let error = sprite(0).generate(&op, &track).await.unwrap_err();

    assert!(matches!(error, SpriteError::NotVideo(_)), "{error}");
}

/// The fixture runs for 1370.32s, so nothing is left to show past it.
#[tokio::test]
async fn generate_refuses_a_time_the_presentation_never_reaches() {
    let (op, track) = probe("video_avc_1080.mp4").await;

    let error = sprite(1_400_000).generate(&op, &track).await.unwrap_err();

    assert!(matches!(error, SpriteError::NotFound(_)), "{error}");
}

fn sprite(time: u64) -> Sprite {
    Sprite {
        tile_size: TILE_SIZE,
        step: STEP,
        time,
    }
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
