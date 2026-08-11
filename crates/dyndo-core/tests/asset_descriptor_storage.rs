use std::sync::Arc;

use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::codec::{CodecConfig, WvttCodec};
use dyndo_core::segment::InitSegment;
use dyndo_core::thumbnail_descriptor::ThumbnailDescriptor;
use dyndo_core::track::Track;
use dyndo_core::track_kind::{TextKind, TrackKind};
use opendal::{Operator, services::Memory};
use relative_path::{RelativePath, RelativePathBuf};

fn memory_operator() -> Operator {
    Operator::new(Memory::default()).unwrap()
}

#[tokio::test]
async fn read_or_new_preserves_the_descriptor_base_when_adding_a_track() {
    let mut descriptor = AssetDescriptor::read_or_new(
        &memory_operator(),
        RelativePath::new("assets/movie/asset.json"),
    )
    .await
    .unwrap();

    let track = Track::new(
        "text".into(),
        RelativePathBuf::from("assets/movie/subtitles/en.vtt"),
        TrackKind::Text(TextKind {
            language: "en".parse().unwrap(),
            role: None,
        }),
        Arc::new(InitSegment::new(CodecConfig::Wvtt(WvttCodec), 1_000, 0, 0)),
        Vec::new(),
    );
    descriptor.add_track(&track);

    assert_eq!(
        descriptor.track_path(descriptor.find_track_by_id("text").unwrap()),
        RelativePath::new("assets/movie/subtitles/en.vtt")
    );
}

#[tokio::test]
async fn read_deserializes_an_asset_descriptor_from_storage() {
    let operator = memory_operator();
    operator.write("assets/asset.json", r#"{"segment_options":{"min_length":1000},"tracks":[{"id":"text","path":"subtitles/en.vtt","codec":"wvtt","type":"text"}],"thumbnails":[{"id":"preview","tile_size":4,"width":640,"step":1000}]}"#).await.unwrap();

    let descriptor = AssetDescriptor::read(&operator, "assets/asset.json")
        .await
        .unwrap();

    assert_eq!(
        descriptor.track_path(descriptor.find_track_by_id("text").unwrap()),
        RelativePath::new("assets/subtitles/en.vtt")
    );
    assert_eq!(
        descriptor.find_thumbnail_by_id("preview"),
        Some(&ThumbnailDescriptor {
            id: "preview".to_string(),
            tile_size: 4,
            width: 640,
            step: 1_000,
        })
    );
}

#[tokio::test]
async fn read_or_new_propagates_invalid_json() {
    let operator = memory_operator();
    operator.write("asset.json", "not json").await.unwrap();

    let result = AssetDescriptor::read_or_new(&operator, RelativePath::new("asset.json")).await;

    assert!(result.is_err());
}
