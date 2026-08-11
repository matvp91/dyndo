use std::sync::Arc;

use dyndo_core::asset::descriptor::ThumbnailTrackDescriptor;
use dyndo_core::asset::kind::{AudioKind, TextKind, ThumbnailKind, VideoKind};
use dyndo_core::codec::{AacCodec, AvcCodec, CodecConfig, WvttCodec};
use dyndo_core::segment::{InitSegment, Segment};
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::cmaf::CmafTrack;
use dyndo_core::track::kind::CmafTrackKind;
use dyndo_core::track::synthetic::SyntheticTrack;
use dyndo_hls::{
    generate_image_playlist, generate_master_playlist, generate_media_playlist, options::HlsOptions,
};
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

fn video_track() -> CmafTrack {
    track(
        "video-main",
        CmafTrackKind::Video(VideoKind {
            width: 16,
            height: 16,
            frame_rate: "4/1".into(),
        }),
        avc_codec(),
        100,
    )
}

fn rendition_tracks() -> Vec<CmafTrack> {
    vec![
        video_track(),
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
    ]
}

fn generate(tracks: &[CmafTrack], hls_options: &HlsOptions) -> (String, Vec<String>) {
    let segment_options = SegmentOptions::default();
    let master = generate_master_playlist(tracks, &[], &segment_options, hls_options).unwrap();
    let media = tracks
        .iter()
        .map(|track| generate_media_playlist(track, &segment_options, hls_options).unwrap())
        .collect();
    (master, media)
}

fn thumbnail(step: u32) -> ThumbnailTrackDescriptor {
    ThumbnailTrackDescriptor {
        id: "preview".to_string(),
        kind: ThumbnailKind {
            tile_size: 2,
            width: 16,
            step,
        },
    }
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

#[test]
fn generated_image_playlists_advertise_existing_thumbnail_sprites() {
    let tracks = [video_track()];
    let descriptor = thumbnail(1_000);
    let preview = SyntheticTrack::thumbnail(&descriptor, &tracks).unwrap();
    let alternate_descriptor = ThumbnailTrackDescriptor {
        id: "alternate".to_string(),
        ..thumbnail(500)
    };
    let alternate = SyntheticTrack::thumbnail(&alternate_descriptor, &tracks).unwrap();
    let master = generate_master_playlist(
        &tracks,
        &[preview, alternate],
        &SegmentOptions::default(),
        &HlsOptions::default(),
    )
    .unwrap();
    let thumbnail = SyntheticTrack::thumbnail(&descriptor, &tracks).unwrap();
    let images = generate_image_playlist(&thumbnail).unwrap();

    assert!(master.contains(
        "#EXT-X-IMAGE-STREAM-INF:BANDWIDTH=64,CODECS=\"jpeg\",RESOLUTION=8x8,URI=\"image_preview.m3u8\""
    ));
    assert!(master.contains(
        "#EXT-X-IMAGE-STREAM-INF:BANDWIDTH=128,CODECS=\"jpeg\",RESOLUTION=8x8,URI=\"image_alternate.m3u8\""
    ));
    assert_eq!(
        images,
        concat!(
            "#EXTM3U\n",
            "#EXT-X-VERSION:6\n",
            "#EXT-X-TARGETDURATION:2\n",
            "#EXT-X-PLAYLIST-TYPE:VOD\n",
            "#EXT-X-IMAGES-ONLY\n",
            "#EXT-X-TILES:RESOLUTION=8x8,LAYOUT=2x2,DURATION=1.000\n",
            "#EXTINF:2,\n",
            "image_preview/0.jpg\n",
            "#EXT-X-ENDLIST\n",
        )
    );
}

#[test]
fn generated_image_playlist_shortens_the_final_sprite() {
    let track = video_track();
    let descriptor = thumbnail(400);
    let thumbnail = SyntheticTrack::thumbnail(&descriptor, std::slice::from_ref(&track)).unwrap();
    let playlist = generate_image_playlist(&thumbnail).unwrap();

    assert!(playlist.contains(concat!(
        "#EXT-X-TILES:RESOLUTION=8x8,LAYOUT=2x2,DURATION=0.400\n",
        "#EXTINF:0.4,\n",
        "image_preview/1600.jpg\n",
    )));
}
