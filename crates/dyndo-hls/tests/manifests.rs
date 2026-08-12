use std::sync::Arc;

use dyndo_core::asset::ResolvedAsset;
use dyndo_core::codec::{AacCodec, AvcCodec, CodecConfig};
use dyndo_core::track::ResolvedTrack;
use dyndo_core::track::cmaf::{CmafKind, InitSegment, ResolvedCmafTrack, Segment};
use dyndo_core::track::metadata::{AudioMetadata, TextMetadata, VideoMetadata};
use dyndo_core::track::thumbnail::ThumbnailTrack;
use dyndo_core::track::timed_text::ResolvedTimedTextTrack;
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

fn track(
    id: &str,
    kind: CmafKind,
    codec: CodecConfig,
    bytes_per_segment: u64,
) -> ResolvedCmafTrack {
    let init = Arc::new(InitSegment::new(codec, 1_000, 0, 100));
    ResolvedCmafTrack::new(
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

fn video_track() -> ResolvedCmafTrack {
    track(
        "video-main",
        CmafKind::Video(VideoMetadata {
            width: 16,
            height: 16,
            frame_rate: "4/1".into(),
        }),
        avc_codec(),
        100,
    )
}

fn rendition_tracks() -> Vec<ResolvedTrack> {
    vec![
        ResolvedTrack::Cmaf(video_track()),
        ResolvedTrack::Cmaf(track(
            "audio-en",
            CmafKind::Audio(AudioMetadata {
                sample_rate: 48_000,
                channels: 2,
                language: "en".parse().unwrap(),
                role: None,
            }),
            aac_codec(),
            50,
        )),
        ResolvedTrack::TimedText(ResolvedTimedTextTrack::from_web_vtt_text(
            "text-en".to_string(),
            "text-en.vtt".into(),
            TextMetadata {
                language: "en".parse().unwrap(),
                role: None,
            },
            "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nHello\n\n00:00:01.000 --> 00:00:02.000\nWorld\n",
        )
        .unwrap()),
    ]
}

async fn generate(tracks: &[ResolvedTrack], hls_options: &HlsOptions) -> (String, Vec<String>) {
    let asset = ResolvedAsset::new(Vec::new(), tracks.to_vec());
    let master = generate_master_playlist(&asset, 0, 1_000, hls_options)
        .await
        .unwrap();
    let mut media = Vec::new();
    for track in tracks {
        media.push(
            generate_media_playlist(track, 0, 1_000, asset.boundaries(), hls_options)
                .await
                .unwrap(),
        );
    }
    (master, media)
}

fn thumbnail(step: u32) -> ThumbnailTrack {
    ThumbnailTrack::new("preview".to_string(), 2, 16, step)
}

#[tokio::test]
async fn generated_two_segment_video_manifests_match_the_golden_fixtures() {
    let (master, media) = generate(
        &[ResolvedTrack::Cmaf(video_track())],
        &HlsOptions::default(),
    )
    .await;

    assert_eq!(
        (master.as_str(), media[0].as_str()),
        (
            include_str!("fixtures/video/master.m3u8"),
            include_str!("fixtures/video/video-main.m3u8"),
        )
    );
}

#[tokio::test]
async fn generated_plain_webvtt_renditions_match_the_golden_fixtures() {
    let (master, media) = generate(&rendition_tracks(), &HlsOptions::default()).await;

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

#[tokio::test]
async fn generated_packaged_wvtt_renditions_match_the_golden_fixtures() {
    let (master, media) = generate(&rendition_tracks(), &HlsOptions { wvtt: true }).await;

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

#[tokio::test]
async fn generated_image_playlists_advertise_existing_thumbnail_sprites() {
    let tracks = [video_track()];
    let configured = thumbnail(1_000);
    let preview = configured.resolve(&tracks).unwrap();
    let alternate = ThumbnailTrack::new("alternate".to_string(), 2, 16, 500);
    let alternate = alternate.resolve(&tracks).unwrap();
    let asset = ResolvedAsset::new(
        Vec::new(),
        vec![
            ResolvedTrack::Cmaf(tracks[0].clone()),
            ResolvedTrack::Thumbnail(preview),
            ResolvedTrack::Thumbnail(alternate),
        ],
    );
    let master = generate_master_playlist(&asset, 0, 0, &HlsOptions::default())
        .await
        .unwrap();
    let thumbnail = configured.resolve(&tracks).unwrap();
    let images = generate_image_playlist(&thumbnail).unwrap();

    assert!(master.contains(
        "#EXT-X-IMAGE-STREAM-INF:BANDWIDTH=64,CODECS=\"jpeg\",RESOLUTION=8x8,URI=\"preview.m3u8\""
    ));
    assert!(master.contains(
        "#EXT-X-IMAGE-STREAM-INF:BANDWIDTH=128,CODECS=\"jpeg\",RESOLUTION=8x8,URI=\"alternate.m3u8\""
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
            "preview/0.jpg\n",
            "#EXT-X-ENDLIST\n",
        )
    );
}

#[test]
fn generated_image_playlist_shortens_the_final_sprite() {
    let track = video_track();
    let configured = thumbnail(400);
    let thumbnail = configured.resolve(std::slice::from_ref(&track)).unwrap();
    let playlist = generate_image_playlist(&thumbnail).unwrap();

    assert!(playlist.contains(concat!(
        "#EXT-X-TILES:RESOLUTION=8x8,LAYOUT=2x2,DURATION=0.400\n",
        "#EXTINF:0.4,\n",
        "preview/1600.jpg\n",
    )));
}
