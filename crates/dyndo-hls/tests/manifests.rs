use std::sync::Arc;

use dyndo_core::codec::{AacCodec, AvcCodec, CodecConfig, WvttCodec};
use dyndo_core::segment::{InitSegment, Segment};
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::Track;
use dyndo_core::track_kind::{AudioKind, TextKind, TrackKind, VideoKind};
use dyndo_hls::{generate_master_playlist, generate_media_playlist, options::HlsOptions};
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

fn video_track() -> Track {
    track(
        "video-main",
        TrackKind::Video(VideoKind {
            width: 16,
            height: 16,
            frame_rate: "4/1".into(),
        }),
        avc_codec(),
        100,
    )
}

fn rendition_tracks() -> Vec<Track> {
    vec![
        video_track(),
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
    ]
}

fn generate(tracks: &[Track], hls_options: &HlsOptions) -> (String, Vec<String>) {
    let segment_options = SegmentOptions::default();
    let master = generate_master_playlist(tracks, &segment_options, hls_options)
        .unwrap()
        .to_string();
    let media = tracks
        .iter()
        .map(|track| {
            generate_media_playlist(track, &segment_options, hls_options)
                .unwrap()
                .to_string()
        })
        .collect();
    (master, media)
}

#[test]
fn generated_two_segment_video_manifests_match_the_golden_fixtures() {
    let (master, media) = generate(&[video_track()], &HlsOptions::default());

    assert_eq!(
        (master.as_str(), media[0].as_str()),
        (
            include_str!("fixtures/video/master.m3u8"),
            include_str!("fixtures/video/video-main.m3u8"),
        )
    );
}

#[test]
fn generated_plain_webvtt_renditions_match_the_golden_fixtures() {
    let (master, media) = generate(&rendition_tracks(), &HlsOptions::default());

    assert_eq!(
        (
            master.as_str(),
            media[0].as_str(),
            media[1].as_str(),
            media[2].as_str(),
        ),
        (
            include_str!("fixtures/video-audio-text/plain-vtt/master.m3u8"),
            include_str!("fixtures/video-audio-text/plain-vtt/video-main.m3u8"),
            include_str!("fixtures/video-audio-text/plain-vtt/audio-en.m3u8"),
            include_str!("fixtures/video-audio-text/plain-vtt/text-en.m3u8"),
        )
    );
}

#[test]
fn generated_packaged_wvtt_renditions_match_the_golden_fixtures() {
    let (master, media) = generate(&rendition_tracks(), &HlsOptions { wvtt: true });

    assert_eq!(
        (
            master.as_str(),
            media[0].as_str(),
            media[1].as_str(),
            media[2].as_str(),
        ),
        (
            include_str!("fixtures/video-audio-text/wvtt/master.m3u8"),
            include_str!("fixtures/video-audio-text/wvtt/video-main.m3u8"),
            include_str!("fixtures/video-audio-text/wvtt/audio-en.m3u8"),
            include_str!("fixtures/video-audio-text/wvtt/text-en.m3u8"),
        )
    );
}
