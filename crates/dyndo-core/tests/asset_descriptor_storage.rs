use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::source_track::SourceTrack;
use dyndo_core::thumbnail_track_descriptor::ThumbnailTrackDescriptor;
use opendal::{Operator, services::Memory};
use relative_path::RelativePath;

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

    let operator = memory_operator();
    operator
        .write(
            "assets/movie/subtitles/en.vtt",
            "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nHello\n",
        )
        .await
        .unwrap();
    let track_descriptor = serde_json::from_str(
        r#"{"id":"text","path":"subtitles/en.vtt","type":"vtt","language":"en"}"#,
    )
    .unwrap();
    let track = SourceTrack::probe(
        &operator,
        RelativePath::new("assets/movie/subtitles/en.vtt"),
        Some(&track_descriptor),
    )
    .await
    .unwrap();
    descriptor.add_source_track(&track);

    assert_eq!(
        descriptor
            .track_path(descriptor.find_track_by_id("text").unwrap())
            .as_deref(),
        Some(RelativePath::new("assets/movie/subtitles/en.vtt"))
    );

    let value = serde_json::to_value(&descriptor).unwrap();
    assert_eq!(value["tracks"][0]["type"], "vtt");
    assert!(value["tracks"][0].get("codec").is_none());
}

#[tokio::test]
async fn read_deserializes_an_asset_descriptor_from_storage() {
    let operator = memory_operator();
    operator.write("assets/asset.json", r#"{"segment_options":{"min_length":1000},"tracks":[{"id":"text","path":"subtitles/en.vtt","type":"vtt"},{"id":"preview","tile_size":4,"width":640,"step":1000,"type":"thumbnail"}]}"#).await.unwrap();

    let descriptor = AssetDescriptor::read(&operator, "assets/asset.json")
        .await
        .unwrap();

    assert_eq!(
        descriptor
            .track_path(descriptor.find_track_by_id("text").unwrap())
            .as_deref(),
        Some(RelativePath::new("assets/subtitles/en.vtt"))
    );
    assert_eq!(
        descriptor.find_thumbnail_by_id("preview"),
        Some(&ThumbnailTrackDescriptor {
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

#[tokio::test]
async fn read_rejects_the_removed_thumbnails_collection() {
    let operator = memory_operator();
    operator
        .write(
            "asset.json",
            r#"{"tracks":[],"thumbnails":[{"id":"preview"}]}"#,
        )
        .await
        .unwrap();

    let result = AssetDescriptor::read(&operator, "asset.json").await;

    assert!(result.is_err());
}

#[tokio::test]
async fn read_rejects_the_removed_image_track_type() {
    let operator = memory_operator();
    operator
        .write(
            "asset.json",
            r#"{"tracks":[{"id":"preview","type":"image","tile_size":4,"width":640,"step":1000}]}"#,
        )
        .await
        .unwrap();

    let result = AssetDescriptor::read(&operator, "asset.json").await;

    assert!(result.is_err());
}
