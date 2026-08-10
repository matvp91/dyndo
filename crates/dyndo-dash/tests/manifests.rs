use std::sync::Arc;

use dyndo_core::codec::{AacCodec, AvcCodec, CodecConfig, WvttCodec};
use dyndo_core::segment::{InitSegment, Segment};
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::Track;
use dyndo_core::track_kind::{AudioKind, TextKind, TrackKind, VideoKind};
use dyndo_dash::{generate_mpd, options::DashOptions};
use mp4_atom::{Audio, Avc1, Avcc, Mp4a};

fn avc_codec() -> CodecConfig {
    CodecConfig::Avc(AvcCodec::new(&Avc1 {
        avcc: Avcc {
            avc_profile_indication: 0x42,
            avc_level_indication: 0x1e,
            length_size: 4,
            ..Avcc::default()
        },
        ..Avc1::default()
    }))
}

fn aac_codec() -> CodecConfig {
    let mut codec = Mp4a {
        audio: Audio {
            data_reference_index: 1,
            channel_count: 2,
            sample_size: 16,
            sample_rate: 48_000.into(),
        },
        esds: Default::default(),
        btrt: None,
        taic: None,
    };
    codec.esds.es_desc.dec_config.dec_specific.profile = 2;
    CodecConfig::Aac(AacCodec::new(&codec))
}

fn track(id: &str, kind: TrackKind, codec: CodecConfig, bytes_per_segment: u64) -> Track {
    let init = Arc::new(InitSegment::new(codec, 1_000, 0, 100));
    Track::new(
        id.into(),
        format!("{id}.mp4").into(),
        kind,
        Arc::clone(&init),
        vec![
            Segment::new(Arc::clone(&init), 0, 1_000, 100, 100 + bytes_per_segment),
            Segment::new(init, 1_000, 2_000, 200, 200 + bytes_per_segment),
        ],
    )
}

fn video_track(id: &str, width: u32, height: u32, bytes_per_segment: u64) -> Track {
    track(
        id,
        TrackKind::Video(VideoKind {
            width,
            height,
            frame_rate: "4/1".into(),
        }),
        avc_codec(),
        bytes_per_segment,
    )
}

fn generate(
    tracks: &[Track],
    segment_options: &SegmentOptions,
    dash_options: &DashOptions,
) -> String {
    let mpd = generate_mpd(tracks, segment_options, dash_options).unwrap();
    quick_xml::se::to_string(&mpd).unwrap()
}

#[test]
fn generated_two_segment_video_mpd_matches_the_golden_fixture() {
    let xml = generate(
        &[video_track("video-main", 16, 16, 100)],
        &SegmentOptions::default(),
        &DashOptions::default(),
    );

    assert_eq!(xml, include_str!("fixtures/video.mpd").trim_end());
}

#[test]
fn generated_compact_mpd_matches_the_golden_fixture() {
    let xml = generate(
        &[video_track("video-main", 16, 16, 100)],
        &SegmentOptions::default(),
        &DashOptions {
            compact: true,
            ..DashOptions::default()
        },
    );

    assert_eq!(xml, include_str!("fixtures/compact.mpd").trim_end());
}

#[test]
fn generated_thumbnail_mpd_addresses_sprites_by_start_time() {
    let xml = generate(
        &[video_track("video-main", 16, 16, 100)],
        &SegmentOptions::default(),
        &DashOptions {
            thumbnail_tile_size: 2,
            thumbnail_step: 1_000,
            ..DashOptions::default()
        },
    );

    assert!(xml.contains(
        "media=\"video-main/$Time$.jpg\" timescale=\"1000\" presentationTimeOffset=\"0\"><SegmentTimeline><S t=\"0\" d=\"4000\"/></SegmentTimeline>"
    ));
}

#[test]
fn generated_multi_period_mpd_matches_the_golden_fixture() {
    let xml = generate(
        &[video_track("video-main", 16, 16, 100)],
        &SegmentOptions {
            boundaries: vec![1_000],
            ..SegmentOptions::default()
        },
        &DashOptions {
            multi_period: true,
            ..DashOptions::default()
        },
    );

    assert_eq!(xml, include_str!("fixtures/multi-period.mpd").trim_end());
}

#[test]
fn generated_grouped_rendition_mpd_matches_the_golden_fixture() {
    let tracks = vec![
        video_track("video-low", 16, 16, 100),
        video_track("video-high", 32, 32, 200),
        track(
            "audio-en",
            TrackKind::Audio(AudioKind {
                sample_rate: 48_000,
                channels: 2,
                language: "en".parse().unwrap(),
                role: None,
            }),
            aac_codec(),
            50,
        ),
        track(
            "text-en",
            TrackKind::Text(TextKind {
                language: "en".parse().unwrap(),
                role: None,
            }),
            CodecConfig::Wvtt(WvttCodec),
            25,
        ),
    ];
    let xml = generate(&tracks, &SegmentOptions::default(), &DashOptions::default());

    assert_eq!(xml, include_str!("fixtures/grouped.mpd").trim_end());
}
