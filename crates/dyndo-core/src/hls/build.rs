use std::time::Duration;

use hls_m3u8::tags::{ExtXMap, ExtXMedia, VariantStream};
use hls_m3u8::types::{Channels, MediaType, PlaylistType, StreamData, UFloat};
use hls_m3u8::{MasterPlaylist, MediaPlaylist, MediaSegment};

use super::group::{self, AudioGroup};
use crate::asset::Asset;
use crate::metadata::VideoMetadata;
use crate::role::AudioRole;
use crate::segment::Segment;
use crate::track::Track;

/// Build the VOD media playlist for `track`: an `EXT-X-MAP` init on the first
/// segment, then one segment per served (sub)segment — the raw CMAF
/// fragments grouped under the asset's grouping pair — named by its running
/// presentation time. `EXT-X-TARGETDURATION` is the longest segment in whole
/// seconds.
pub(super) fn build_media(
    track: &Track,
    boundaries_ms: &[u64],
    min_length_ms: u64,
) -> MediaPlaylist<'static> {
    let repr = track.id();
    let timescale = track.timescale();
    let served = track.segments(boundaries_ms, min_length_ms);

    let mut time = track.earliest_presentation_time();
    let mut segments: Vec<MediaSegment<'static>> = Vec::with_capacity(served.len());
    for (i, seg) in served.iter().enumerate() {
        let mut b = MediaSegment::builder();
        b.uri(format!("{repr}/{time}.m4s"));
        b.duration(Duration::from_secs_f64(
            seg.duration as f64 / timescale as f64,
        ));
        if i == 0 {
            b.map(ExtXMap::new(format!("{repr}/init.mp4")));
        }
        segments.push(
            b.build()
                .expect("a media segment always has a URI and duration"),
        );
        time += seg.duration;
    }

    let mut b = MediaPlaylist::builder();
    b.media_sequence(0);
    b.target_duration(Duration::from_secs(target_duration(&served, timescale)));
    b.playlist_type(PlaylistType::Vod);
    b.has_end_list(true);
    b.segments(segments);
    b.build()
        .expect("target duration covers the longest segment")
}

/// The longest segment in whole seconds, rounded up (RFC 8216 requires an
/// integer `EXT-X-TARGETDURATION` ≥ every segment's rounded duration).
fn target_duration(segments: &[Segment], timescale: u32) -> u64 {
    segments
        .iter()
        .map(|s| (s.duration as f64 / timescale as f64).ceil() as u64)
        .max()
        .unwrap_or(1)
}

/// Build the multivariant playlist. Video tracks become `EXT-X-STREAM-INF`
/// variants (one per audio group, cartesian); audio tracks become
/// `EXT-X-MEDIA` renditions. With no video, audio tracks are the variants
/// (no group); with no audio, variants carry no `AUDIO`. Text and raw
/// (non-CMAF) tracks are not advertised.
pub(super) fn build_master(asset: &Asset) -> MasterPlaylist<'static> {
    let videos = group::videos(asset);
    let audios = group::audios(asset);
    let groups = group::audio_group(&audios);

    // Audio tracks are `EXT-X-MEDIA` renditions only when there is video;
    // with no video the audio tracks are themselves the variants instead.
    let mut media: Vec<ExtXMedia<'static>> = Vec::new();
    let variants: Vec<VariantStream<'static>> = if videos.is_empty() {
        audios.iter().map(|&(t, _)| audio_variant(t)).collect()
    } else {
        media.extend(audio_media(&groups));
        video_variants(&videos, &groups)
    };

    let mut b = MasterPlaylist::builder();
    b.media(media);
    b.variant_streams(variants);
    b.has_independent_segments(true);
    b.build()
        .expect("every variant references a defined audio group")
}

/// Audio roles the viewer opts into deliberately — never auto-selected on a
/// language match.
fn is_opt_in_audio(role: Option<AudioRole>) -> bool {
    matches!(
        role,
        Some(
            AudioRole::Commentary
                | AudioRole::Description
                | AudioRole::EnhancedAudioIntelligibility
        )
    )
}

/// The `CHARACTERISTICS` UTI(s) for an audio role, if any.
fn audio_characteristics(role: Option<AudioRole>) -> Option<&'static str> {
    match role {
        Some(AudioRole::Description) => Some("public.accessibility.describes-video"),
        Some(AudioRole::EnhancedAudioIntelligibility) => {
            Some("public.accessibility.enhances-speech-intelligibility")
        }
        _ => None,
    }
}

/// One `EXT-X-MEDIA` per audio track. The group default is its first
/// `main`-role rendition, or the first rendition when no member declares
/// `main` — so a role-free group keeps the "first is default" behavior.
/// Roles the viewer opts into (commentary, description, enhanced
/// intelligibility) are not auto-selected, and accessibility roles carry a
/// `CHARACTERISTICS` UTI.
fn audio_media(groups: &[AudioGroup]) -> Vec<ExtXMedia<'static>> {
    let mut out = Vec::new();
    for g in groups {
        let default_idx = g
            .tracks
            .iter()
            .position(|(_, a)| a.role == Some(AudioRole::Main))
            .unwrap_or(0);
        for (i, &(t, a)) in g.tracks.iter().enumerate() {
            let is_default = i == default_idx;
            let mut b = ExtXMedia::builder();
            b.media_type(MediaType::Audio);
            b.group_id(g.id.clone());
            b.name(a.language.clone());
            b.language(a.language.clone());
            b.uri(format!("{}.m3u8", t.id()));
            b.is_default(is_default);
            // Opt-in roles are not auto-selected — unless this rendition is
            // the group default, since DEFAULT=YES requires AUTOSELECT=YES.
            b.is_autoselect(is_default || !is_opt_in_audio(a.role));
            b.channels(Channels::new(a.channels as u64));
            if let Some(c) = audio_characteristics(a.role) {
                b.characteristics(c);
            }
            out.push(
                b.build()
                    .expect("audio media always has a type, group id, and name"),
            );
        }
    }
    out
}

/// Every video track × every audio group (or just the video track when there
/// are no groups).
fn video_variants(
    videos: &[(&Track, &VideoMetadata)],
    groups: &[AudioGroup],
) -> Vec<VariantStream<'static>> {
    let mut out = Vec::new();
    for &(t, v) in videos {
        let (num, den) = t.frame_rate();
        let fr = (num != 0).then(|| num as f32 / den as f32);
        if groups.is_empty() {
            out.push(video_variant(t, v, fr, None));
        } else {
            for g in groups {
                out.push(video_variant(t, v, fr, Some(g)));
            }
        }
    }
    out
}

fn video_variant(
    t: &Track,
    v: &VideoMetadata,
    fr: Option<f32>,
    group: Option<&AudioGroup>,
) -> VariantStream<'static> {
    let mut codecs = vec![t.codec().expect("video tracks are CMAF").to_string()];
    let mut bandwidth = t.bandwidth() as u64;
    let audio = group.map(|g| {
        codecs.push(g.codec.clone());
        bandwidth += g.max_bandwidth as u64;
        g.id.clone().into()
    });
    VariantStream::ExtXStreamInf {
        uri: format!("{}.m3u8", t.id()).into(),
        frame_rate: fr.map(UFloat::new),
        audio,
        subtitles: None,
        closed_captions: None,
        stream_data: StreamData::builder()
            .bandwidth(bandwidth)
            .codecs(codecs)
            .resolution((v.width as usize, v.height as usize))
            .build()
            .expect("stream data always has a bandwidth"),
    }
}

/// A standalone audio variant, used only when the asset has no video.
fn audio_variant(t: &Track) -> VariantStream<'static> {
    VariantStream::ExtXStreamInf {
        uri: format!("{}.m3u8", t.id()).into(),
        frame_rate: None,
        audio: None,
        subtitles: None,
        closed_captions: None,
        stream_data: StreamData::builder()
            .bandwidth(t.bandwidth() as u64)
            .codecs(vec![t.codec().expect("audio tracks are CMAF").to_string()])
            .build()
            .expect("stream data always has a bandwidth"),
    }
}
