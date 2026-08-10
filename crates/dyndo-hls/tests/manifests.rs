use std::sync::Arc;

use dyndo_core::codec::{AvcCodec, CodecConfig};
use dyndo_core::segment::{InitSegment, Segment};
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::Track;
use dyndo_core::track_kind::{TrackKind, VideoKind};
use dyndo_hls::{generate_master_playlist, generate_media_playlist, options::HlsOptions};
use mp4_atom::{Avc1, Avcc};

fn video_track() -> Track {
    let codec = CodecConfig::Avc(AvcCodec::new(&Avc1 {
        avcc: Avcc {
            avc_profile_indication: 0x42,
            avc_level_indication: 0x1e,
            length_size: 4,
            ..Avcc::default()
        },
        ..Avc1::default()
    }));
    let init = Arc::new(InitSegment::new(codec, 1_000, 0, 100));
    Track::new(
        "video-main".into(),
        "video.mp4".into(),
        TrackKind::Video(VideoKind {
            width: 16,
            height: 16,
            frame_rate: "4/1".into(),
        }),
        Arc::clone(&init),
        vec![Segment::new(init, 0, 1_000, 100, 200)],
    )
}

#[test]
fn generated_video_manifests_match_the_golden_fixtures() {
    let track = video_track();
    let options = SegmentOptions::default();
    let hls_options = HlsOptions::default();
    let master = generate_master_playlist(std::slice::from_ref(&track), &options, &hls_options)
        .unwrap()
        .to_string();
    let media = generate_media_playlist(&track, &options, &hls_options)
        .unwrap()
        .to_string();

    assert_eq!(master, include_str!("fixtures/video-master.m3u8"));
    assert_eq!(media, include_str!("fixtures/video-media.m3u8"));
}
