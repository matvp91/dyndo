use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::cmaf_track_kind::CmafTrackKind;
use dyndo_core::probe::probe_tracks;
use dyndo_core::reader::Reader;
use dyndo_core::text::Subtitle;
use dyndo_core::track::Track;
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
                    {"id":"text-en","path":"subtitles/en.vtt","type":"vtt","language":"en"},
                    {"id":"preview","type":"image","tile_size":4,"width":640,"step":1000}
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
    assert_eq!(tracks.len(), 3);
    assert!(matches!(tracks.last(), Some(Track::Thumbnail(_))));
    let video = tracks
        .iter()
        .find(|track| track.id() == "video-main")
        .and_then(Track::native_cmaf)
        .unwrap();
    let subtitles = tracks
        .iter()
        .find(|track| track.id() == "text-en")
        .and_then(Track::vtt)
        .unwrap();
    let packaged_subtitles = subtitles.package(&asset.segment_options).await.unwrap();
    let subtitle_end = packaged_subtitles
        .cmaf()
        .segments()
        .last()
        .unwrap()
        .byte_range()
        .end;
    let video_initialization = Reader::new(&operator)
        .read_initialization(video)
        .await
        .unwrap();

    assert!(matches!(video.kind(), CmafTrackKind::Video(_)));
    assert_eq!(
        video_initialization.as_ref(),
        &VIDEO_FIXTURE[..video.init_segment().byte_range().end as usize]
    );
    assert_eq!(
        Subtitle::from_wvtt(&packaged_subtitles.read(0..subtitle_end).unwrap())
            .unwrap()
            .cues
            .len(),
        1
    );
}
