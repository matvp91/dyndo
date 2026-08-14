use std::sync::Arc;

use dyndo_core::asset::ResolvedAsset;
use dyndo_core::codec::{AacCodec, AvcCodec, CodecConfig, WvttCodec};
use dyndo_core::drm::CpixParser;
use dyndo_core::track::ResolvedTrack;
use dyndo_core::track::cmaf::{CmafMetadata, InitSegment, ResolvedCmafTrack, Segment};
use dyndo_core::track::metadata::{AudioMetadata, TextMetadata, VideoMetadata};
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

fn track(
    id: &str,
    metadata: CmafMetadata,
    codec: CodecConfig,
    bytes_per_segment: u64,
) -> ResolvedCmafTrack {
    let init = Arc::new(InitSegment::new(codec, 1_000, 0, 100));
    ResolvedCmafTrack::new(
        id.into(),
        format!("{id}.mp4").into(),
        metadata,
        Arc::clone(&init),
        vec![
            Segment::new(Arc::clone(&init), 0, 1_000, 100, 100 + bytes_per_segment),
            Segment::new(init, 1_000, 2_000, 200, 200 + bytes_per_segment),
        ],
    )
}

fn video_track(id: &str, width: u32, height: u32, bytes_per_segment: u64) -> ResolvedCmafTrack {
    track(
        id,
        CmafMetadata::Video(VideoMetadata {
            width,
            height,
            frame_rate: "4/1".into(),
        }),
        avc_codec(),
        bytes_per_segment,
    )
}

fn video_track_with_segment_count(segment_count: u32) -> ResolvedCmafTrack {
    let init = Arc::new(InitSegment::new(avc_codec(), 1_000, 0, 100));
    let segments = (0..segment_count)
        .map(|index| {
            let start = u64::from(index) * 1_000;
            Segment::new(
                Arc::clone(&init),
                start,
                start + 1_000,
                100 + start,
                1_100 + start,
            )
        })
        .collect();
    ResolvedCmafTrack::new(
        "video-main".to_string(),
        "video-main.mp4".into(),
        CmafMetadata::Video(VideoMetadata {
            width: 16,
            height: 16,
            frame_rate: "4/1".into(),
        }),
        init,
        segments,
    )
}

async fn generate(
    tracks: &[ResolvedCmafTrack],
    thumbnail_tracks: &[ThumbnailTrack],
    min_length: u32,
    text_length: u32,
    boundaries: &[u32],
    dash_options: &DashOptions,
) -> String {
    let sources: Vec<_> = tracks.iter().cloned().map(Arc::new).collect();
    let thumbnails: Vec<_> = thumbnail_tracks
        .iter()
        .filter_map(|thumbnail| thumbnail.resolve(sources.iter().cloned()))
        .collect();
    let mut resolved_tracks: Vec<_> = sources.into_iter().map(ResolvedTrack::Cmaf).collect();
    resolved_tracks.extend(thumbnails.into_iter().map(ResolvedTrack::Thumbnail));
    let asset = ResolvedAsset::new(boundaries.to_vec(), resolved_tracks);
    generate_mpd(&asset, min_length, text_length, dash_options)
        .await
        .unwrap()
        .lines()
        .skip(1)
        .map(str::trim)
        .collect()
}

fn thumbnail() -> ThumbnailTrack {
    ThumbnailTrack::new("preview".to_string(), 2, 16)
}

#[tokio::test]
async fn generated_two_segment_video_mpd_matches_the_golden_fixture() {
    let xml = generate(
        &[video_track("video-main", 16, 16, 100)],
        &[],
        0,
        0,
        &[],
        &DashOptions::default(),
    )
    .await;

    assert_eq!(xml, include_str!("fixtures/video.mpd").trim_end());
}

#[tokio::test]
async fn protected_video_mpd_uses_resolved_content_protection() {
    let cpix = CpixParser::parse(include_str!("../../../assets/cpix_mk.xml")).unwrap();
    let track = video_track("video-main", 16, 16, 100)
        .with_protection(&cpix)
        .unwrap();
    let xml = generate(&[track], &[], 0, 0, &[], &DashOptions::default()).await;

    assert!(xml.contains("schemeIdUri=\"urn:mpeg:dash:mp4protection:2011\" value=\"cenc\""));
    assert!(xml.contains("cenc:default_KID=\"abba271e-8bcf-552b-bd2e-86a434a9a5d9\""));
    assert!(xml.contains("schemeIdUri=\"urn:uuid:edef8ba9-79d6-4ace-a3c8-27dcd51d21ed\""));
}

#[tokio::test]
async fn generated_compact_mpd_matches_the_golden_fixture() {
    let xml = generate(
        &[video_track("video-main", 16, 16, 100)],
        &[],
        0,
        0,
        &[],
        &DashOptions {
            compact: true,
            ..DashOptions::default()
        },
    )
    .await;

    assert_eq!(xml, include_str!("fixtures/compact.mpd").trim_end());
}

#[tokio::test]
async fn generated_thumbnail_mpd_addresses_sprites_by_number() {
    let xml = generate(
        &[video_track("video-main", 16, 16, 100)],
        &[thumbnail()],
        0,
        0,
        &[],
        &DashOptions::default(),
    )
    .await;

    assert!(xml.contains(
        "media=\"preview/$Number$.jpg\" startNumber=\"1\" timescale=\"1000\" presentationTimeOffset=\"0\"><SegmentTimeline><S t=\"0\" d=\"4000\"/></SegmentTimeline>"
    ));
}

#[tokio::test]
async fn generated_multi_period_mpd_matches_the_golden_fixture() {
    let xml = generate(
        &[video_track("video-main", 16, 16, 100)],
        &[],
        0,
        0,
        &[1_000],
        &DashOptions {
            multi_period: true,
            ..DashOptions::default()
        },
    )
    .await;

    assert_eq!(xml, include_str!("fixtures/multi-period.mpd").trim_end());
}

#[tokio::test]
async fn generated_multi_period_mpd_slides_templates_by_the_millisecond_boundary() {
    let xml = generate(
        &[video_track("video-main", 16, 16, 100)],
        &[thumbnail()],
        0,
        0,
        &[750],
        &DashOptions {
            multi_period: true,
            ..DashOptions::default()
        },
    )
    .await;

    assert!(xml.contains(
        "<Period id=\"1\" start=\"PT0.75S\" duration=\"PT1.25S\"><AdaptationSet id=\"0\" contentType=\"video\" mimeType=\"video/mp4\" startWithSAP=\"1\"><SupplementalProperty schemeIdUri=\"urn:mpeg:dash:period-connectivity:2015\" value=\"0\"/><Representation id=\"video-main\" bandwidth=\"800\" width=\"16\" height=\"16\" frameRate=\"4/1\" codecs=\"avc1.42001e\"><SegmentTemplate media=\"$RepresentationID$/$Time$.m4s\" initialization=\"$RepresentationID$/init.mp4\" timescale=\"1000\" presentationTimeOffset=\"750\"><SegmentTimeline><S t=\"0\" d=\"1000\" r=\"1\"/></SegmentTimeline>"
    ));
}

#[tokio::test]
async fn generated_multi_period_mpd_preserves_a_boundary_crossing_thumbnail_number() {
    let xml = generate(
        &[video_track("video-main", 16, 16, 100)],
        &[thumbnail()],
        0,
        0,
        &[1_000],
        &DashOptions {
            multi_period: true,
            ..DashOptions::default()
        },
    )
    .await;

    assert!(xml.contains(
        "contentType=\"image\" mimeType=\"image/jpeg\"><SupplementalProperty schemeIdUri=\"urn:mpeg:dash:period-connectivity:2015\" value=\"0\"/><Representation id=\"preview\" bandwidth=\"64\" width=\"16\" height=\"16\"><EssentialProperty schemeIdUri=\"http://dashif.org/guidelines/thumbnail_tile\" value=\"2x2\"/><SegmentTemplate media=\"preview/$Number$.jpg\" startNumber=\"1\" timescale=\"1000\" presentationTimeOffset=\"1000\"><SegmentTimeline><S t=\"0\" d=\"4000\"/></SegmentTimeline>"
    ));
}

#[tokio::test]
async fn generated_multi_period_mpd_uses_the_global_number_for_a_later_overlapping_sprite() {
    let xml = generate(
        &[video_track_with_segment_count(100)],
        &[thumbnail()],
        0,
        0,
        &[90_000],
        &DashOptions {
            multi_period: true,
            ..DashOptions::default()
        },
    )
    .await;

    assert!(xml.contains(
        "<AdaptationSet id=\"1\" contentType=\"image\" mimeType=\"image/jpeg\"><SupplementalProperty schemeIdUri=\"urn:mpeg:dash:period-connectivity:2015\" value=\"0\"/><Representation id=\"preview\" bandwidth=\"64\" width=\"16\" height=\"16\"><EssentialProperty schemeIdUri=\"http://dashif.org/guidelines/thumbnail_tile\" value=\"2x2\"/><SegmentTemplate media=\"preview/$Number$.jpg\" startNumber=\"23\" timescale=\"1000\" presentationTimeOffset=\"90000\"><SegmentTimeline><S t=\"88000\" d=\"4000\" r=\"2\"/></SegmentTimeline>"
    ));
}

#[tokio::test]
async fn generated_grouped_rendition_mpd_matches_the_golden_fixture() {
    let tracks = vec![
        video_track("video-low", 16, 16, 100),
        video_track("video-high", 32, 32, 200),
        track(
            "audio-en",
            CmafMetadata::Audio(AudioMetadata {
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
            CmafMetadata::Text(TextMetadata {
                language: "en".parse().unwrap(),
                role: None,
            }),
            CodecConfig::Wvtt(WvttCodec),
            25,
        ),
    ];
    let xml = generate(&tracks, &[], 0, 0, &[], &DashOptions::default()).await;

    assert_eq!(xml, include_str!("fixtures/grouped.mpd").trim_end());
}
