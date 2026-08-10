use dyndo_core::asset_descriptor::AssetDescriptor;
use opendal::{Operator, services::Memory};
use relative_path::RelativePath;

fn memory_operator() -> Operator {
    Operator::new(Memory::default()).unwrap()
}

#[tokio::test]
async fn read_or_new_returns_an_empty_descriptor_when_the_descriptor_is_missing() {
    let descriptor = AssetDescriptor::read_or_new(
        &memory_operator(),
        RelativePath::new("assets/movie/asset.json"),
    )
    .await
    .unwrap();

    assert!(descriptor.tracks.is_empty());
}

#[tokio::test]
async fn read_deserializes_an_asset_descriptor_from_storage() {
    let operator = memory_operator();
    operator.write("assets/asset.json", r#"{"segment_options":{"min_length":1000},"tracks":[{"id":"text","path":"subtitles/en.vtt","codec":"wvtt","type":"text"}]}"#).await.unwrap();

    let descriptor = AssetDescriptor::read(&operator, "assets/asset.json")
        .await
        .unwrap();

    assert_eq!(
        descriptor.track_path(descriptor.find_track_by_id("text").unwrap()),
        RelativePath::new("assets/subtitles/en.vtt")
    );
}

#[tokio::test]
async fn read_or_new_propagates_invalid_json() {
    let operator = memory_operator();
    operator.write("asset.json", "not json").await.unwrap();

    let result = AssetDescriptor::read_or_new(&operator, RelativePath::new("asset.json")).await;

    assert!(result.is_err());
}
