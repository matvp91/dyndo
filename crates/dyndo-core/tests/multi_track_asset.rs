use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::probe::probe_tracks;
use dyndo_core::reader::Reader;
use dyndo_core::text::Subtitle;
use dyndo_core::track_kind::TrackKind;
use opendal::{Operator, services::Memory};

const VIDEO_FIXTURE: &[u8] = include_bytes!("fixtures/three-frame-black-h264.mp4");
const VTT: &str = "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nHello\n";

fn memory_operator() -> Operator {
    Operator::new(Memory::default()).unwrap()
}

#[tokio::test]
async fn probe_tracks_and_readers_serve_the_video_and_subtitle_tracks_of_one_asset() {
    let operator = memory_operator();
    operator
        .write(
            "assets/movie/asset.json",
            r#"{
                "segment_options":{"text_length":1000},
                "tracks":[
                    {"id":"video-main","path":"video.mp4","codec":"avc1.42c00a","type":"video","width":16,"height":16,"frame_rate":"4/1"},
                    {"id":"text-en","path":"subtitles/en.vtt","codec":"wvtt","type":"text","language":"en"}
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

    let asset = AssetDescriptor::read(&operator, "assets/movie/asset.json")
        .await
        .unwrap();
    let tracks = probe_tracks(&operator, &asset).await.unwrap();
    let video = tracks
        .iter()
        .find(|track| track.id() == "video-main")
        .unwrap();
    let subtitles = tracks.iter().find(|track| track.id() == "text-en").unwrap();
    let video_reader = Reader::new(&operator, video, &asset.segment_options);
    let text_reader = Reader::new(&operator, subtitles, &asset.segment_options);
    let video_initialization = video_reader.read_initialization().await.unwrap();
    let text_media = text_reader
        .read_range(0..subtitles.segments().last().unwrap().byte_range().end)
        .await
        .unwrap();

    assert!(matches!(video.kind(), TrackKind::Video(_)));
    assert_eq!(
        video_initialization.as_ref(),
        &VIDEO_FIXTURE[..video.init_segment().byte_range().end as usize]
    );
    assert!(matches!(subtitles.kind(), TrackKind::Text(_)));
    assert_eq!(Subtitle::from_wvtt(&text_media).unwrap().cues.len(), 1);
}
