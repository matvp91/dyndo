use std::time::Duration;

use hls_m3u8::tags::{ExtXMap, ExtXMedia, VariantStream};
use hls_m3u8::types::{Channels, MediaType, PlaylistType, StreamData, UFloat};
use hls_m3u8::{MasterPlaylist, MediaPlaylist, MediaSegment};

use crate::asset::{AudioTrack, Segment, Track, VideoTrack};

/// Build the VOD media playlist for `track`: an `EXT-X-MAP` init on the first
/// segment, then one segment per (sub)segment named by its running presentation
/// time. `EXT-X-TARGETDURATION` is the longest segment in whole seconds.
pub(crate) fn build_media<T: Track>(track: &T) -> MediaPlaylist<'static> {
    let repr = track.id();
    let header = track.cmaf_header();
    let timescale = header.timescale;

    let mut time = header.earliest_presentation_time;
    let mut segments: Vec<MediaSegment<'static>> = Vec::with_capacity(header.segments.len());
    for (i, seg) in header.segments.iter().enumerate() {
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
    b.target_duration(Duration::from_secs(target_duration(
        &header.segments,
        timescale,
    )));
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

/// Audio tracks sharing one codec fourcc → one `EXT-X-MEDIA` `GROUP-ID`.
struct AudioGroup<'a> {
    /// `GROUP-ID` = the codec fourcc (`"mp4a"`, `"ec-3"`, …).
    id: &'static str,
    /// A representative RFC 6381 string for the group's `CODECS` contribution.
    codec: String,
    /// The highest-bandwidth member's bandwidth, added to a variant's `BANDWIDTH`.
    max_bandwidth: u32,
    /// The group's renditions in first-seen order; the first is the default.
    tracks: Vec<&'a AudioTrack>,
}

/// Build the multivariant playlist. Video tracks become `EXT-X-STREAM-INF`
/// variants (one per audio group, cartesian); audio tracks become `EXT-X-MEDIA`
/// renditions. With no video, audio tracks are the variants (no group); with no
/// audio, variants carry no `AUDIO`.
pub(crate) fn build_master(
    videos: &[VideoTrack],
    audios: &[AudioTrack],
) -> MasterPlaylist<'static> {
    let groups = group_by_codec(audios);

    let (media, variants): (Vec<ExtXMedia<'static>>, Vec<VariantStream<'static>>) =
        if videos.is_empty() {
            (Vec::new(), audios.iter().map(audio_variant).collect())
        } else {
            (audio_media(&groups), video_variants(videos, &groups))
        };

    let mut b = MasterPlaylist::builder();
    b.media(media);
    b.variant_streams(variants);
    b.has_independent_segments(true);
    b.build()
        .expect("every variant references a defined audio group")
}

/// Group audio tracks by codec fourcc, preserving first-seen order.
fn group_by_codec(audios: &[AudioTrack]) -> Vec<AudioGroup<'_>> {
    let mut groups: Vec<AudioGroup> = Vec::new();
    for t in audios {
        let fourcc = t.cmaf_metadata.codec.fourcc();
        match groups.iter_mut().find(|g| g.id == fourcc) {
            Some(g) => {
                g.max_bandwidth = g.max_bandwidth.max(t.cmaf_header.bandwidth);
                g.tracks.push(t);
            }
            None => groups.push(AudioGroup {
                id: fourcc,
                codec: t.cmaf_metadata.codec.rfc6381(),
                max_bandwidth: t.cmaf_header.bandwidth,
                tracks: vec![t],
            }),
        }
    }
    groups
}

/// One `EXT-X-MEDIA` per audio track. The first rendition in each group is the
/// group default.
fn audio_media(groups: &[AudioGroup]) -> Vec<ExtXMedia<'static>> {
    let mut out = Vec::new();
    for g in groups {
        for (i, &t) in g.tracks.iter().enumerate() {
            let m = &t.cmaf_metadata;
            let mut b = ExtXMedia::builder();
            b.media_type(MediaType::Audio);
            b.group_id(g.id);
            b.name(m.language.clone());
            b.language(m.language.clone());
            b.uri(format!("{}.m3u8", t.id()));
            b.is_default(i == 0);
            b.is_autoselect(true);
            b.channels(Channels::new(m.channels as u64));
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
fn video_variants(videos: &[VideoTrack], groups: &[AudioGroup]) -> Vec<VariantStream<'static>> {
    let mut out = Vec::new();
    for v in videos {
        let (num, den) = v.cmaf_metadata.frame_rate;
        let fr = (num != 0).then(|| num as f32 / den as f32);
        if groups.is_empty() {
            out.push(video_variant(v, fr, None));
        } else {
            for g in groups {
                out.push(video_variant(v, fr, Some(g)));
            }
        }
    }
    out
}

fn video_variant(
    v: &VideoTrack,
    fr: Option<f32>,
    group: Option<&AudioGroup>,
) -> VariantStream<'static> {
    let m = &v.cmaf_metadata;
    let mut codecs = vec![m.codec.rfc6381()];
    let mut bandwidth = v.cmaf_header.bandwidth as u64;
    let audio = group.map(|g| {
        codecs.push(g.codec.clone());
        bandwidth += g.max_bandwidth as u64;
        g.id.into()
    });
    VariantStream::ExtXStreamInf {
        uri: format!("{}.m3u8", v.id()).into(),
        frame_rate: fr.map(UFloat::new),
        audio,
        subtitles: None,
        closed_captions: None,
        stream_data: StreamData::builder()
            .bandwidth(bandwidth)
            .codecs(codecs)
            .resolution((m.width as usize, m.height as usize))
            .build()
            .expect("stream data always has a bandwidth"),
    }
}

/// A standalone audio variant, used only when the asset has no video.
fn audio_variant(t: &AudioTrack) -> VariantStream<'static> {
    let m = &t.cmaf_metadata;
    VariantStream::ExtXStreamInf {
        uri: format!("{}.m3u8", t.id()).into(),
        frame_rate: None,
        audio: None,
        subtitles: None,
        closed_captions: None,
        stream_data: StreamData::builder()
            .bandwidth(t.cmaf_header.bandwidth as u64)
            .codecs(vec![m.codec.rfc6381()])
            .build()
            .expect("stream data always has a bandwidth"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmaf::{AudioCmafMetadata, CmafHeader, VideoCmafMetadata};
    use crate::codec::{AudioCodec, VideoCodec};

    /// A [`CmafHeader`] with `bandwidth` and one `Segment` per entry in `segs`
    /// (each carrying only a duration; offsets/sizes are irrelevant to playlists).
    fn cmaf_header(timescale: u32, bandwidth: u32, segs: &[u64]) -> CmafHeader {
        CmafHeader {
            timescale,
            duration: segs.iter().sum(),
            bandwidth,
            earliest_presentation_time: 0,
            init_segment: Segment {
                offset: 0,
                size: 0,
                duration: 0,
            },
            segments: segs
                .iter()
                .map(|&d| Segment {
                    offset: 0,
                    size: 0,
                    duration: d,
                })
                .collect(),
        }
    }

    fn video_track(height: u32, bandwidth: u32, segs: &[u64]) -> VideoTrack {
        VideoTrack {
            path: String::new(),
            cmaf_header: cmaf_header(90_000, bandwidth, segs),
            cmaf_metadata: VideoCmafMetadata {
                codec: VideoCodec::Avc {
                    profile: 0x64,
                    constraints: 0x00,
                    level: 0x28,
                },
                width: height * 16 / 9,
                height,
                frame_rate: (25, 1),
            },
        }
    }

    fn audio_track(
        codec: AudioCodec,
        lang: &str,
        channels: u16,
        bandwidth: u32,
        segs: &[u64],
    ) -> AudioTrack {
        AudioTrack {
            path: String::new(),
            cmaf_header: cmaf_header(48_000, bandwidth, segs),
            cmaf_metadata: AudioCmafMetadata {
                codec,
                sample_rate: 48_000,
                channels,
                language: lang.to_string(),
            },
        }
    }

    #[test]
    fn media_playlist_has_vod_map_and_running_time_segments() {
        // 90_000 timescale; segments 2s, 2s, 1s → presentation times 0, 180000, 360000.
        let track = video_track(720, 128_000, &[180_000, 180_000, 90_000]);
        let repr = track.id();
        let m = build_media(&track).to_string();

        assert!(m.contains("#EXT-X-PLAYLIST-TYPE:VOD"), "{m}");
        assert!(m.contains("#EXT-X-TARGETDURATION:2"), "{m}");
        assert_eq!(m.matches("#EXT-X-MAP").count(), 1, "{m}");
        assert!(
            m.contains(&format!("#EXT-X-MAP:URI=\"{repr}/init.mp4\"")),
            "{m}"
        );
        assert!(m.contains(&format!("{repr}/0.m4s")), "{m}");
        assert!(m.contains(&format!("{repr}/180000.m4s")), "{m}");
        assert!(m.contains(&format!("{repr}/360000.m4s")), "{m}");
        // 180_000 units @ 90_000 = 2s → EXTINF derives duration from the
        // timescale, so this catches a timescale/duration miscalculation that
        // a URI-only check would miss.
        assert!(m.contains("#EXTINF:2,"), "{m}");
        assert!(m.contains("#EXTINF:1,"), "{m}");
        assert!(m.contains("#EXT-X-ENDLIST"), "{m}");
    }

    #[test]
    fn media_segment_uris_reflect_nonzero_presentation_time() {
        // Nonzero eps: the first segment starts at eps, not 0. 90_000 timescale;
        // eps 45000, segments 2s, 1s → presentation times 45000, 225000.
        let mut track = video_track(720, 128_000, &[180_000, 90_000]);
        track.cmaf_header.earliest_presentation_time = 45_000;
        let repr = track.id();
        let m = build_media(&track).to_string();

        // First segment named by eps itself, not 0.
        assert!(m.contains(&format!("{repr}/45000.m4s")), "{m}");
        assert!(!m.contains(&format!("{repr}/0.m4s")), "{m}");
        // Second segment: eps + first duration = 45000 + 180000 = 225000.
        assert!(m.contains(&format!("{repr}/225000.m4s")), "{m}");
        // The EXT-X-MAP init still rides the first (offset) segment only.
        assert_eq!(m.matches("#EXT-X-MAP").count(), 1, "{m}");
    }

    #[test]
    fn target_duration_rounds_fractional_segment_up() {
        // 135_000 units @ 90_000 = 1.5s → ceil → 2 (proves .ceil() rounds up
        // rather than truncating to 1).
        let track = video_track(720, 128_000, &[135_000]);
        let m = build_media(&track).to_string();
        assert!(m.contains("#EXT-X-TARGETDURATION:2"), "{m}");
    }

    #[test]
    fn master_pairs_video_variant_with_audio_group() {
        let v = video_track(1080, 4_000_000, &[180_000]);
        let a = audio_track(
            AudioCodec::Aac {
                audio_object_type: 2,
            },
            "nld",
            2,
            128_000,
            &[96_000],
        );
        let (vid, aid) = (v.id(), a.id());
        let m = build_master(&[v], &[a]).to_string();

        assert!(m.contains("#EXT-X-INDEPENDENT-SEGMENTS"), "{m}");
        assert!(m.contains("#EXT-X-MEDIA:TYPE=AUDIO"), "{m}");
        assert!(m.contains("GROUP-ID=\"mp4a\""), "{m}");
        assert!(m.contains("LANGUAGE=\"nld\""), "{m}");
        assert!(m.contains(&format!("URI=\"{aid}.m3u8\"")), "{m}");
        assert!(m.contains("#EXT-X-STREAM-INF:"), "{m}");
        assert!(m.contains("AUDIO=\"mp4a\""), "{m}");
        assert!(m.contains("avc1.640028"), "{m}");
        assert!(m.contains("mp4a.40.2"), "{m}");
        assert!(m.contains("RESOLUTION=1920x1080"), "{m}");
        assert!(m.contains(&format!("{vid}.m3u8")), "{m}");
    }

    #[test]
    fn multiple_video_bitrates_share_one_audio_group() {
        let v1 = video_track(1080, 4_000_000, &[180_000]);
        let v2 = video_track(720, 2_000_000, &[180_000]);
        let a = audio_track(
            AudioCodec::Aac {
                audio_object_type: 2,
            },
            "nld",
            2,
            128_000,
            &[96_000],
        );
        let m = build_master(&[v1, v2], &[a]).to_string();

        assert_eq!(m.matches("#EXT-X-STREAM-INF").count(), 2, "{m}");
        assert_eq!(m.matches("#EXT-X-MEDIA:TYPE=AUDIO").count(), 1, "{m}");
        assert_eq!(m.matches("AUDIO=\"mp4a\"").count(), 2, "{m}");
    }

    #[test]
    fn multiple_audio_codecs_expand_variants() {
        let v = video_track(1080, 4_000_000, &[180_000]);
        let aac = audio_track(
            AudioCodec::Aac {
                audio_object_type: 2,
            },
            "nld",
            2,
            128_000,
            &[96_000],
        );
        let ec3 = audio_track(AudioCodec::Ec3, "nld", 6, 384_000, &[96_000]);
        let m = build_master(&[v], &[aac, ec3]).to_string();

        assert_eq!(m.matches("#EXT-X-STREAM-INF").count(), 2, "{m}");
        assert_eq!(m.matches("#EXT-X-MEDIA:TYPE=AUDIO").count(), 2, "{m}");
        assert!(m.contains("GROUP-ID=\"mp4a\""), "{m}");
        assert!(m.contains("GROUP-ID=\"ec-3\""), "{m}");
        assert!(m.contains("AUDIO=\"mp4a\""), "{m}");
        assert!(m.contains("AUDIO=\"ec-3\""), "{m}");
    }

    #[test]
    fn video_only_has_no_audio_group() {
        let v = video_track(1080, 4_000_000, &[180_000]);
        let m = build_master(&[v], &[]).to_string();

        assert!(m.contains("#EXT-X-STREAM-INF"), "{m}");
        assert!(!m.contains("#EXT-X-MEDIA"), "{m}");
        assert!(!m.contains("AUDIO="), "{m}");
    }

    #[test]
    fn audio_only_lists_audio_as_variants() {
        let a = audio_track(
            AudioCodec::Aac {
                audio_object_type: 2,
            },
            "nld",
            2,
            128_000,
            &[96_000],
        );
        let m = build_master(&[], &[a]).to_string();

        assert!(m.contains("#EXT-X-STREAM-INF"), "{m}");
        assert!(!m.contains("#EXT-X-MEDIA"), "{m}");
        assert!(!m.contains("AUDIO="), "{m}");
        assert!(m.contains("mp4a.40.2"), "{m}");
    }

    #[test]
    fn master_groups_multiple_audio_renditions() {
        // Two audio tracks with the SAME codec but different language/bandwidth
        // collapse into one group (two renditions), and the video variant's
        // BANDWIDTH sums the video bitrate with the group's highest-bandwidth member.
        let v = video_track(1080, 4_000_000, &[180_000]);
        let a_nld = audio_track(
            AudioCodec::Aac {
                audio_object_type: 2,
            },
            "nld",
            2,
            128_000,
            &[96_000],
        );
        let a_eng = audio_track(
            AudioCodec::Aac {
                audio_object_type: 2,
            },
            "eng",
            2,
            96_000,
            &[96_000],
        );
        let m = build_master(&[v], &[a_nld, a_eng]).to_string();

        // Two renditions in the single "mp4a" group.
        assert_eq!(m.matches("#EXT-X-MEDIA:TYPE=AUDIO").count(), 2, "{m}");
        assert!(m.contains("GROUP-ID=\"mp4a\""), "{m}");
        // Exactly one rendition is the group default (the first added: nld).
        assert_eq!(m.matches("DEFAULT=YES").count(), 1, "{m}");
        assert!(m.contains("LANGUAGE=\"nld\""), "{m}");
        assert!(m.contains("LANGUAGE=\"eng\""), "{m}");
        // One video × one group → one variant.
        assert_eq!(m.matches("#EXT-X-STREAM-INF").count(), 1, "{m}");
        // BANDWIDTH = video 4_000_000 + group max audio 128_000 = 4_128_000.
        assert!(m.contains("BANDWIDTH=4128000"), "{m}");
    }
}
