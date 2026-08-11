use dyndo_core::asset::Asset;
use dyndo_core::track::ResolvedTrack;
use dyndo_core::track::cmaf::CmafKind;
use dyndo_core::track::timed_text::TimedTextFormat;
use opendal::{Operator, services::Memory};

const VIDEO_FIXTURE: &[u8] = include_bytes!("fixtures/three-frame-black-h264.mp4");
const VTT: &str = "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nHello\n";

fn memory_operator() -> Operator {
    Operator::new(Memory::default()).unwrap()
}

#[tokio::test]
async fn resolved_asset_contains_video_subtitle_and_thumbnail_tracks() {
    let operator = memory_operator();
    operator
        .write(
            "assets/movie/asset.json",
            r#"{
                "segment_options":{"text_length":1000},
                "tracks":[
                    {"id":"video-main","path":"video.mp4","codec":"avc1.42c00a","type":"video","width":16,"height":16,"frame_rate":"4/1"},
                    {"id":"text-en","path":"subtitles/en.vtt","type":"webvtt","language":"en"},
                    {"id":"preview","type":"thumbnail","tile_size":4,"width":640,"step":1000}
                ]
            }"#,
        )
        .await
        .unwrap();
    operator
        .write("assets/movie/video.mp4", VIDEO_FIXTURE)
        .await
        .unwrap();
    operator
        .write("assets/movie/subtitles/en.vtt", VTT)
        .await
        .unwrap();

    let asset = Asset::read(&operator, "assets/movie/asset.json")
        .await
        .unwrap();
    let resolved = asset.resolve(&operator).await.unwrap();
    assert_eq!(resolved.tracks().len(), 3);
    assert_eq!(resolved.thumbnails().count(), 1);
    let video = resolved
        .track("video-main")
        .and_then(ResolvedTrack::cmaf)
        .unwrap();
    let subtitles = resolved
        .track("text-en")
        .and_then(ResolvedTrack::timed_text)
        .unwrap();
    assert!(matches!(subtitles.format(), TimedTextFormat::WebVtt(_)));
    let packaged_subtitles = subtitles
        .package_wvtt(&asset.segment_options)
        .await
        .unwrap();
    let video_initialization = video
        .read_range(&operator, video.init_segment().byte_range())
        .await
        .unwrap();

    assert!(matches!(video.kind(), CmafKind::Video(_)));
    assert_eq!(
        video_initialization.as_ref(),
        &VIDEO_FIXTURE[..video.init_segment().byte_range().end as usize]
    );
    assert_eq!(packaged_subtitles.segments().len(), 1);
}

#[tokio::test]
async fn asset_resolves_one_track_without_resolving_the_full_asset() {
    let operator = memory_operator();
    operator
        .write(
            "asset.json",
            r#"{
                "tracks":[
                    {"id":"video-main","path":"video.mp4","codec":"avc1.42c00a","type":"video","width":16,"height":16,"frame_rate":"4/1"},
                    {"id":"missing-text","path":"missing.vtt","type":"webvtt","language":"und"}
                ]
            }"#,
        )
        .await
        .unwrap();
    operator.write("video.mp4", VIDEO_FIXTURE).await.unwrap();
    let asset = Asset::read(&operator, "asset.json").await.unwrap();

    let track = asset
        .resolve_track(&operator, "video-main")
        .await
        .unwrap()
        .unwrap();

    assert!(matches!(track, ResolvedTrack::Cmaf(_)));
}
