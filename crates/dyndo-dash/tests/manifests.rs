use std::sync::Arc;

use dyndo_core::asset::thumbnail::ThumbnailTrackDescriptor;
use dyndo_core::codec::{AacCodec, AvcCodec, CodecConfig, WvttCodec};
use dyndo_core::segment::{InitSegment, Segment};
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::cmaf::CmafTrack;
use dyndo_core::track::cmaf::kind::{AudioKind, CmafTrackKind, TextKind, VideoKind};
use dyndo_core::track::thumbnail::ThumbnailTrack;
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

fn track(id: &str, kind: CmafTrackKind, codec: CodecConfig, bytes_per_segment: u64) -> CmafTrack {
    let init = Arc::new(InitSegment::new(codec, 1_000, 0, 100));
    CmafTrack::new(
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

fn video_track(id: &str, width: u32, height: u32, bytes_per_segment: u64) -> CmafTrack {
    track(
        id,
        CmafTrackKind::Video(VideoKind {
            width,
            height,
            frame_rate: "4/1".into(),
        }),
        avc_codec(),
        bytes_per_segment,
    )
}

fn generate(
    tracks: &[CmafTrack],
    descriptors: &[ThumbnailTrackDescriptor],
    segment_options: &SegmentOptions,
    dash_options: &DashOptions,
) -> String {
    let thumbnails: Vec<_> = descriptors
        .iter()
        .filter_map(|descriptor| ThumbnailTrack::new(descriptor, tracks))
        .collect();
    let mpd = generate_mpd(tracks, &thumbnails, segment_options, dash_options).unwrap();
    quick_xml::se::to_string(&mpd).unwrap()
}

fn thumbnail() -> ThumbnailTrackDescriptor {
    ThumbnailTrackDescriptor {
        id: "preview".to_string(),
        tile_size: 2,
        width: 16,
        step: 1_000,
    }
}

fn named_fixture(fixture: &str) -> String {
    fixture
        .replace("id=\"video-main\"", "id=\"video_video-main\"")
        .replace("id=\"video-low\"", "id=\"video_video-low\"")
        .replace("id=\"video-high\"", "id=\"video_video-high\"")
        .replace("id=\"audio-en\"", "id=\"audio_audio-en\"")
        .replace("id=\"text-en\"", "id=\"text_text-en\"")
}

#[test]
fn generated_two_segment_video_mpd_matches_the_golden_fixture() {
    let xml = generate(
        &[video_track("video-main", 16, 16, 100)],
        &[],
        &SegmentOptions::default(),
        &DashOptions::default(),
    );

    assert_eq!(
        xml,
        named_fixture(include_str!("fixtures/video.mpd")).trim_end()
    );
}

#[test]
fn generated_compact_mpd_matches_the_golden_fixture() {
    let xml = generate(
        &[video_track("video-main", 16, 16, 100)],
        &[],
        &SegmentOptions::default(),
        &DashOptions {
            compact: true,
            ..DashOptions::default()
        },
    );

    assert_eq!(
        xml,
        named_fixture(include_str!("fixtures/compact.mpd")).trim_end()
    );
}

#[test]
fn generated_thumbnail_mpd_addresses_sprites_by_start_time() {
    let xml = generate(
        &[video_track("video-main", 16, 16, 100)],
        &[thumbnail()],
        &SegmentOptions::default(),
        &DashOptions::default(),
    );

    assert!(xml.contains(
        "media=\"image_preview/$Time$.jpg\" timescale=\"1000\" presentationTimeOffset=\"0\"><SegmentTimeline><S t=\"0\" d=\"4000\"/></SegmentTimeline>"
    ));
}

#[test]
fn generated_multi_period_mpd_matches_the_golden_fixture() {
    let xml = generate(
        &[video_track("video-main", 16, 16, 100)],
        &[],
        &SegmentOptions {
            boundaries: vec![1_000],
            ..SegmentOptions::default()
        },
        &DashOptions {
            multi_period: true,
            ..DashOptions::default()
        },
    );

    assert_eq!(
        xml,
        named_fixture(include_str!("fixtures/multi-period.mpd")).trim_end()
    );
}

#[test]
fn generated_multi_period_mpd_slides_templates_by_the_millisecond_boundary() {
    let xml = generate(
        &[video_track("video-main", 16, 16, 100)],
        &[thumbnail()],
        &SegmentOptions {
            boundaries: vec![750],
            ..SegmentOptions::default()
        },
        &DashOptions {
            multi_period: true,
            ..DashOptions::default()
        },
    );

    assert!(xml.contains(
        "<Period id=\"1\" start=\"PT0.75S\" duration=\"PT1.25S\"><AdaptationSet id=\"0\" contentType=\"video\" segmentAlignment=\"true\" mimeType=\"video/mp4\" startWithSAP=\"1\"><SupplementalProperty schemeIdUri=\"urn:mpeg:dash:period-connectivity:2015\" value=\"0\"/><Representation id=\"video_video-main\" bandwidth=\"800\" width=\"16\" height=\"16\" frameRate=\"4/1\" codecs=\"avc1.42001e\"><SegmentTemplate media=\"$RepresentationID$/$Time$.m4s\" initialization=\"$RepresentationID$/init.mp4\" timescale=\"1000\" presentationTimeOffset=\"750\"><SegmentTimeline><S t=\"0\" d=\"1000\" r=\"1\"/></SegmentTimeline>"
    ));
}

#[test]
fn generated_multi_period_mpd_references_a_boundary_crossing_thumbnail_sprite_twice() {
    let xml = generate(
        &[video_track("video-main", 16, 16, 100)],
        &[thumbnail()],
        &SegmentOptions {
            boundaries: vec![1_000],
            ..SegmentOptions::default()
        },
        &DashOptions {
            multi_period: true,
            ..DashOptions::default()
        },
    );

    assert!(xml.contains(
        "contentType=\"image\" mimeType=\"image/jpeg\"><SupplementalProperty schemeIdUri=\"urn:mpeg:dash:period-connectivity:2015\" value=\"0\"/><Representation id=\"image_preview\" bandwidth=\"64\" width=\"16\" height=\"16\"><EssentialProperty schemeIdUri=\"http://dashif.org/guidelines/thumbnail_tile\" value=\"2x2\"/><SegmentTemplate media=\"image_preview/$Time$.jpg\" timescale=\"1000\" presentationTimeOffset=\"1000\"><SegmentTimeline><S t=\"0\" d=\"4000\"/></SegmentTimeline>"
    ));
}

#[test]
fn generated_grouped_rendition_mpd_matches_the_golden_fixture() {
    let tracks = vec![
        video_track("video-low", 16, 16, 100),
        video_track("video-high", 32, 32, 200),
        track(
            "audio-en",
            CmafTrackKind::Audio(AudioKind {
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
            CmafTrackKind::Text(TextKind {
                language: "en".parse().unwrap(),
                role: None,
            }),
            CodecConfig::Wvtt(WvttCodec),
            25,
        ),
    ];
    let xml = generate(
        &tracks,
        &[],
        &SegmentOptions::default(),
        &DashOptions::default(),
    );

    assert_eq!(
        xml,
        named_fixture(include_str!("fixtures/grouped.mpd")).trim_end()
    );
}
