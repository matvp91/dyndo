use dyndo_core::asset::Asset;
use dyndo_core::track::SourceTrack;
use dyndo_core::track::Track;
use dyndo_core::track::thumbnail::ThumbnailTrack;
use opendal::{Operator, services::Memory};
use relative_path::{RelativePath, RelativePathBuf};

fn memory_operator() -> Operator {
    Operator::new(Memory::default()).unwrap()
}

#[test]
fn thumbnail_track_serializes_its_type_from_the_track_variant() {
    let track = Track::Thumbnail(ThumbnailTrack::new("preview".to_string(), 4, 640, 1_000));

    let value = serde_json::to_value(track).unwrap();

    assert_eq!(value["type"], "thumbnail");
}

#[tokio::test]
async fn read_or_new_preserves_the_asset_base_when_adding_a_track() {
    let mut asset = Asset::read_or_new(
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
    let source_track: SourceTrack = serde_json::from_str(
        r#"{"id":"text","path":"subtitles/en.vtt","type":"webvtt","language":"en"}"#,
    )
    .unwrap();
    let track = source_track
        .resolve(
            &operator,
            RelativePath::new("assets/movie/subtitles/en.vtt"),
        )
        .await
        .unwrap();
    asset.add_source_track(&track).unwrap();

    assert_eq!(
        asset.track_path(asset.find_source_track_by_id("text").unwrap()),
        RelativePathBuf::from("assets/movie/subtitles/en.vtt")
    );

    asset.write(&operator).await.unwrap();
    let value = serde_json::to_value(
        Asset::read(&operator, "assets/movie/asset.json")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(value["tracks"][0]["type"], "webvtt");
    assert!(value["tracks"][0].get("codec").is_none());
}

#[tokio::test]
async fn read_deserializes_an_asset_from_storage() {
    let operator = memory_operator();
    operator.write("assets/asset.json", r#"{"segment_options":{"min_length":1000},"tracks":[{"id":"text","path":"subtitles/en.vtt","type":"webvtt"},{"id":"preview","tile_size":4,"width":640,"step":1000,"type":"thumbnail"}]}"#).await.unwrap();

    let asset = Asset::read(&operator, "assets/asset.json").await.unwrap();

    assert_eq!(
        asset.track_path(asset.find_source_track_by_id("text").unwrap()),
        RelativePathBuf::from("assets/subtitles/en.vtt")
    );
    assert_eq!(
        asset.find_thumbnail_track_by_id("preview"),
        Some(&ThumbnailTrack::new("preview".to_string(), 4, 640, 1_000))
    );
}

#[tokio::test]
async fn read_or_new_propagates_invalid_json() {
    let operator = memory_operator();
    operator.write("asset.json", "not json").await.unwrap();

    let result = Asset::read_or_new(&operator, RelativePath::new("asset.json")).await;

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

    let result = Asset::read(&operator, "asset.json").await;

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

    let result = Asset::read(&operator, "asset.json").await;

    assert!(result.is_err());
}

#[tokio::test]
async fn read_rejects_the_legacy_vtt_track_type() {
    let operator = memory_operator();
    operator
        .write(
            "asset.json",
            r#"{"tracks":[{"id":"text","path":"subtitles/en.vtt","type":"vtt"}]}"#,
        )
        .await
        .unwrap();

    let result = Asset::read(&operator, "asset.json").await;

    assert!(result.is_err());
}
