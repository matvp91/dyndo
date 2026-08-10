use std::sync::Arc;

use dyndo_core::codec::{AvcCodec, CodecConfig};
use dyndo_core::segment::{InitSegment, Segment};
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::Track;
use dyndo_core::track_kind::{TrackKind, VideoKind};
use dyndo_dash::{generate_mpd, options::DashOptions};
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
fn generated_video_mpd_matches_the_golden_fixture() {
    let mpd = generate_mpd(
        &[video_track()],
        &SegmentOptions::default(),
        &DashOptions::default(),
    )
    .unwrap();
    let xml = quick_xml::se::to_string(&mpd).unwrap();

    assert_eq!(xml, include_str!("fixtures/video.mpd").trim_end());
}
