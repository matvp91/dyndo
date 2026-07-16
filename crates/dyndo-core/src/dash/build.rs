use std::time::Duration;

use dash_mpd::{
    Accessibility, AdaptationSet, AudioChannelConfiguration, MPD, Period, Representation, Role, S,
    SegmentTemplate, SegmentTimeline,
};

use crate::asset::{AudioTrack, Segment, TextTrack, Track, VideoTrack};
use crate::role::{AudioRole, TextRole};

const INIT_TEMPLATE: &str = "$RepresentationID$/init.mp4";
const MEDIA_TEMPLATE: &str = "$RepresentationID$/$Time$.m4s";
const MPD_XMLNS: &str = "urn:mpeg:dash:schema:mpd:2011";
const MPD_PROFILE: &str = "urn:mpeg:dash:profile:isoff-live:2011";
const AUDIO_CHANNEL_CONFIG_SCHEME: &str = "urn:mpeg:dash:23003:3:audio_channel_configuration:2011";
const ROLE_SCHEME: &str = "urn:mpeg:dash:role:2011";
const AUDIO_PURPOSE_SCHEME: &str = "urn:tva:metadata:cs:AudioPurposeCS:2007";

fn frame_rate_str(fr: (u32, u32)) -> String {
    if fr.1 == 1 {
        fr.0.to_string()
    } else {
        format!("{}/{}", fr.0, fr.1)
    }
}

fn build_timeline(segments: &[Segment], first_t: u64) -> Vec<S> {
    let mut out: Vec<S> = Vec::new();
    let mut first = true;
    for seg in segments {
        match out.last_mut() {
            Some(last) if last.d == seg.duration => {
                *last.r.get_or_insert(0) += 1;
            }
            _ => {
                out.push(S {
                    t: if first { Some(first_t) } else { None },
                    d: seg.duration,
                    r: None,
                    ..Default::default()
                });
                first = false;
            }
        }
    }
    out
}

fn segment_template(timescale: u32, pto: u64, segments: &[Segment]) -> SegmentTemplate {
    SegmentTemplate {
        timescale: Some(timescale as u64),
        presentationTimeOffset: Some(pto),
        initialization: Some(INIT_TEMPLATE.to_string()),
        media: Some(MEDIA_TEMPLATE.to_string()),
        SegmentTimeline: Some(SegmentTimeline {
            segments: build_timeline(segments, pto),
        }),
        ..Default::default()
    }
}

/// The fields every representation shares: id, bandwidth, codecs, and the
/// segment template. Per-media-type builders add their own dimensions or audio
/// configuration.
fn base_representation(
    track: &impl Track,
    codecs: String,
    segment_boundaries_ms: &[u64],
    min_segment_length_ms: u64,
) -> Representation {
    let segments = track.segments(segment_boundaries_ms, min_segment_length_ms);
    Representation {
        id: Some(track.id()),
        bandwidth: Some(track.bandwidth() as u64),
        codecs: Some(codecs),
        SegmentTemplate: Some(segment_template(
            track.timescale(),
            track.earliest_presentation_time(),
            &segments,
        )),
        ..Default::default()
    }
}

fn video_representation(
    track: &VideoTrack,
    segment_boundaries_ms: &[u64],
    min_segment_length_ms: u64,
) -> Representation {
    Representation {
        width: Some(track.width() as u64),
        height: Some(track.height() as u64),
        frameRate: (track.frame_rate().0 != 0).then(|| frame_rate_str(track.frame_rate())),
        ..base_representation(
            track,
            track.codec().rfc6381(),
            segment_boundaries_ms,
            min_segment_length_ms,
        )
    }
}

fn audio_representation(
    track: &AudioTrack,
    segment_boundaries_ms: &[u64],
    min_segment_length_ms: u64,
) -> Representation {
    Representation {
        audioSamplingRate: Some(track.sample_rate().to_string()),
        AudioChannelConfiguration: vec![AudioChannelConfiguration {
            schemeIdUri: AUDIO_CHANNEL_CONFIG_SCHEME.to_string(),
            value: Some(track.channels().to_string()),
            ..Default::default()
        }],
        ..base_representation(
            track,
            track.codec().rfc6381(),
            segment_boundaries_ms,
            min_segment_length_ms,
        )
    }
}

/// A text `Representation` carries no dimensions or channel configuration; the
/// shared base (id, bandwidth, `codecs`, segment template) is all it needs.
fn text_representation(
    track: &TextTrack,
    segment_boundaries_ms: &[u64],
    min_segment_length_ms: u64,
) -> Representation {
    base_representation(
        track,
        track.codec().rfc6381(),
        segment_boundaries_ms,
        min_segment_length_ms,
    )
}

/// A single `Role` descriptor in the standard role scheme.
fn role_element(value: &str) -> Role {
    Role {
        schemeIdUri: ROLE_SCHEME.to_string(),
        value: Some(value.to_string()),
        ..Default::default()
    }
}

/// The `Role`(s) for a text track. An unset role defaults to `subtitle`,
/// preserving the previous hardcoded behavior.
fn text_roles(role: Option<TextRole>) -> Vec<Role> {
    vec![role_element(role.map_or("subtitle", TextRole::as_str))]
}

/// The `Role`(s) for an audio track. An unset role emits none.
fn audio_roles(role: Option<AudioRole>) -> Vec<Role> {
    role.map(|r| vec![role_element(r.as_str())])
        .unwrap_or_default()
}

/// The `Accessibility` descriptor(s) for an audio track — present only for the
/// audio-description and enhanced-intelligibility roles (AudioPurposeCS).
fn audio_accessibility(role: Option<AudioRole>) -> Vec<Accessibility> {
    let value = match role {
        Some(AudioRole::Description) => "1",
        Some(AudioRole::EnhancedAudioIntelligibility) => "8",
        _ => return Vec::new(),
    };
    vec![Accessibility {
        schemeIdUri: AUDIO_PURPOSE_SCHEME.to_string(),
        value: Some(value.to_string()),
        id: None,
    }]
}

/// Group track indices by a key, preserving first-seen order of keys and members.
fn group_by_key<T, K: PartialEq>(items: &[T], key: impl Fn(&T) -> K) -> Vec<(K, Vec<usize>)> {
    let mut groups: Vec<(K, Vec<usize>)> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let k = key(item);
        match groups.iter_mut().find(|(existing, _)| *existing == k) {
            Some((_, idxs)) => idxs.push(i),
            None => groups.push((k, vec![i])),
        }
    }
    groups
}

/// Build one `AdaptationSet` from a group of like representations. `roles` and
/// `accessibility` are the DASH `Role`/`Accessibility` descriptors for the set
/// (derived from the group's track role); video sets pass none.
fn adaptation_set(
    id: usize,
    content_type: &str,
    mime: &str,
    lang: Option<String>,
    roles: Vec<Role>,
    accessibility: Vec<Accessibility>,
    representations: Vec<Representation>,
) -> AdaptationSet {
    AdaptationSet {
        id: Some(id.to_string()),
        contentType: Some(content_type.to_string()),
        mimeType: Some(mime.to_string()),
        lang,
        segmentAlignment: Some(true),
        startWithSAP: Some(1),
        Accessibility: accessibility,
        Role: roles,
        representations,
        ..Default::default()
    }
}

/// Build the raw static VOD `MPD`: one `AdaptationSet` per video codec fourcc,
/// then one per audio `(fourcc, language, role)` and per text
/// `(fourcc, language, role)`, each track becoming a `Representation`.
fn mpd(
    videos: &[VideoTrack],
    audios: &[AudioTrack],
    texts: &[TextTrack],
    segment_boundaries_ms: &[u64],
    min_segment_length_ms: u64,
) -> MPD {
    let mut adaptations = Vec::new();
    let mut set_id = 0;

    for (_fourcc, idxs) in group_by_key(videos, |t| t.codec().fourcc()) {
        let representations = idxs
            .iter()
            .map(|&i| {
                video_representation(&videos[i], segment_boundaries_ms, min_segment_length_ms)
            })
            .collect();
        adaptations.push(adaptation_set(
            set_id,
            "video",
            "video/mp4",
            None,
            Vec::new(),
            Vec::new(),
            representations,
        ));
        set_id += 1;
    }
    for ((_fourcc, lang, role), idxs) in group_by_key(audios, |t| {
        (t.codec().fourcc(), t.language().to_string(), t.role())
    }) {
        let representations = idxs
            .iter()
            .map(|&i| {
                audio_representation(&audios[i], segment_boundaries_ms, min_segment_length_ms)
            })
            .collect();
        adaptations.push(adaptation_set(
            set_id,
            "audio",
            "audio/mp4",
            Some(lang),
            audio_roles(role),
            audio_accessibility(role),
            representations,
        ));
        set_id += 1;
    }
    for ((_fourcc, lang, role), idxs) in group_by_key(texts, |t| {
        (t.codec().fourcc(), t.language().to_string(), t.role())
    }) {
        let representations = idxs
            .iter()
            .map(|&i| text_representation(&texts[i], segment_boundaries_ms, min_segment_length_ms))
            .collect();
        adaptations.push(adaptation_set(
            set_id,
            "text",
            "application/mp4",
            Some(lang),
            text_roles(role),
            Vec::new(),
            representations,
        ));
        set_id += 1;
    }

    let period = Period {
        id: Some("0".to_string()),
        start: Some(Duration::ZERO),
        adaptations,
        ..Default::default()
    };

    let min_buffer_ms = videos
        .iter()
        .map(|t| t.max_segment_duration_ms(segment_boundaries_ms, min_segment_length_ms))
        .chain(
            audios
                .iter()
                .map(|t| t.max_segment_duration_ms(segment_boundaries_ms, min_segment_length_ms)),
        )
        .chain(
            texts
                .iter()
                .map(|t| t.max_segment_duration_ms(segment_boundaries_ms, min_segment_length_ms)),
        )
        .max()
        .unwrap_or(0);
    let media_duration_ms = videos
        .iter()
        .map(|t| t.duration_ms())
        .chain(audios.iter().map(|t| t.duration_ms()))
        .chain(texts.iter().map(|t| t.duration_ms()))
        .max()
        .unwrap_or(0);

    MPD {
        xmlns: Some(MPD_XMLNS.to_string()),
        mpdtype: Some("static".to_string()),
        profiles: Some(MPD_PROFILE.to_string()),
        minBufferTime: Some(Duration::from_millis(min_buffer_ms)),
        mediaPresentationDuration: Some(Duration::from_millis(media_duration_ms)),
        periods: vec![period],
        ..Default::default()
    }
}

/// Assemble the final `MPD`, optionally hoisting shared `SegmentTemplate` content
/// up to the `AdaptationSet` level.
pub(crate) fn build_mpd(
    videos: &[VideoTrack],
    audios: &[AudioTrack],
    texts: &[TextTrack],
    segment_boundaries_ms: &[u64],
    min_segment_length_ms: u64,
    compact: bool,
) -> MPD {
    let mut m = mpd(
        videos,
        audios,
        texts,
        segment_boundaries_ms,
        min_segment_length_ms,
    );
    if compact {
        super::compact::compact(&mut m);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmaf::{AudioCmafMetadata, CmafHeader, TextCmafMetadata};
    use crate::codec::{AudioCodec, TextCodec};
    use crate::role::{AudioRole, TextRole};

    fn seg(duration: u64) -> Segment {
        Segment {
            offset: 0,
            size: 0,
            duration,
            duration_ms: duration,
        }
    }

    fn text_track_with_segments(language: &str, segments: Vec<Segment>) -> TextTrack {
        let duration = segments.iter().map(|s| s.duration).sum();
        TextTrack::new(
            String::new(),
            CmafHeader {
                timescale: 1000,
                duration,
                bandwidth: 256,
                earliest_presentation_time: 0,
                init_segment: seg(0),
                segments,
            },
            TextCmafMetadata {
                codec: TextCodec::Wvtt,
                language: Some(language.to_string()),
            },
            None,
        )
    }

    fn text_track(language: &str) -> TextTrack {
        text_track_with_segments(language, vec![seg(4000)])
    }

    #[test]
    fn mpd_advertises_text_adaptation_set_with_subtitle_role() {
        let m = mpd(&[], &[], &[text_track("eng")], &[], 0);
        let text = m.periods[0]
            .adaptations
            .iter()
            .find(|a| a.contentType.as_deref() == Some("text"))
            .expect("a text AdaptationSet");

        assert_eq!(text.mimeType.as_deref(), Some("application/mp4"));
        assert_eq!(text.lang.as_deref(), Some("eng"));
        assert_eq!(text.Role.len(), 1);
        assert_eq!(text.Role[0].schemeIdUri, ROLE_SCHEME);
        assert_eq!(text.Role[0].value.as_deref(), Some("subtitle"));

        let rep = &text.representations[0];
        assert_eq!(rep.id.as_deref(), Some("text_wvtt_eng"));
        assert_eq!(rep.codecs.as_deref(), Some("wvtt"));
    }

    #[test]
    fn mpd_emits_one_text_adaptation_set_per_language() {
        let m = mpd(&[], &[], &[text_track("eng"), text_track("nld")], &[], 0);
        let text_sets = m.periods[0]
            .adaptations
            .iter()
            .filter(|a| a.contentType.as_deref() == Some("text"))
            .count();
        assert_eq!(text_sets, 2);
    }

    #[test]
    fn build_timeline_sets_time_only_on_the_first_entry() {
        let out = build_timeline(&[seg(10), seg(20)], 100);
        assert_eq!(out[0].t, Some(100));
        assert_eq!(out[1].t, None);
    }

    #[test]
    fn build_timeline_run_length_encodes_equal_durations() {
        // DASH S@r counts *additional* repeats, so three equal segments
        // collapse to a single S with r = 2.
        let out = build_timeline(&[seg(10), seg(10), seg(10)], 0);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].r, Some(2));
    }

    #[test]
    fn build_timeline_starts_a_new_entry_on_duration_change() {
        let out = build_timeline(&[seg(10), seg(20)], 0);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].d, 20);
    }

    #[test]
    fn frame_rate_str_omits_a_denominator_of_one() {
        assert_eq!(frame_rate_str((30, 1)), "30");
    }

    #[test]
    fn frame_rate_str_keeps_a_fractional_rate() {
        assert_eq!(frame_rate_str((30_000, 1_001)), "30000/1001");
    }

    #[test]
    fn group_by_key_preserves_first_seen_key_order() {
        let items = ["a", "b", "a", "c"];
        let groups = group_by_key(&items, |s| s.to_string());
        let keys: Vec<_> = groups.iter().map(|(k, _)| k.clone()).collect();
        assert_eq!(keys, vec!["a", "b", "c"]);
    }

    #[test]
    fn group_by_key_collects_indices_of_like_keys() {
        let items = ["a", "b", "a"];
        let groups = group_by_key(&items, |s| s.to_string());
        assert_eq!(groups[0].1, vec![0, 2]);
    }

    #[test]
    fn mpd_timeline_groups_segments_by_the_policy() {
        let track = text_track_with_segments("eng", vec![seg(1000); 4]);
        let m = mpd(&[], &[], &[track], &[], 2000);
        let tmpl = m.periods[0].adaptations[0].representations[0]
            .SegmentTemplate
            .as_ref()
            .unwrap();
        let timeline = tmpl.SegmentTimeline.as_ref().unwrap();
        assert_eq!(timeline.segments.len(), 1);
        assert_eq!(timeline.segments[0].d, 2000);
        assert_eq!(timeline.segments[0].r, Some(1)); // two 2s segments
    }

    #[test]
    fn mpd_min_buffer_time_reflects_grouped_segments() {
        let track = text_track_with_segments("eng", vec![seg(1000); 4]);
        let m = mpd(&[], &[], &[track], &[], 2000);
        assert_eq!(
            m.minBufferTime,
            Some(std::time::Duration::from_millis(2000))
        );
    }

    fn text_track_role(language: &str, role: TextRole) -> TextTrack {
        let model = crate::model::TextTrackModel {
            id: format!("text_wvtt_{language}"),
            path: String::new(),
            fourcc: "wvtt".to_string(),
            timescale: 1000,
            language: language.to_string(),
            role: Some(role),
        };
        TextTrack::new(
            String::new(),
            CmafHeader {
                timescale: 1000,
                duration: 4000,
                bandwidth: 256,
                earliest_presentation_time: 0,
                init_segment: seg(0),
                segments: vec![seg(4000)],
            },
            TextCmafMetadata {
                codec: TextCodec::Wvtt,
                language: Some(language.to_string()),
            },
            Some(&model),
        )
    }

    fn audio_track_role(language: &str, role: Option<AudioRole>) -> AudioTrack {
        let model = crate::model::AudioTrackModel {
            id: format!("audio_{language}"),
            path: String::new(),
            fourcc: "mp4a".to_string(),
            timescale: 48_000,
            sample_rate: 48_000,
            channels: 2,
            language: Some(language.to_string()),
            role,
        };
        AudioTrack::new(
            String::new(),
            CmafHeader {
                timescale: 48_000,
                duration: 96_000,
                bandwidth: 128_000,
                earliest_presentation_time: 0,
                init_segment: seg(0),
                segments: vec![seg(96_000)],
            },
            AudioCmafMetadata {
                codec: AudioCodec::Aac {
                    audio_object_type: 2,
                },
                sample_rate: 48_000,
                channels: 2,
                language: Some(language.to_string()),
            },
            Some(&model),
        )
    }

    fn text_set<'a>(m: &'a MPD, lang: &str) -> &'a AdaptationSet {
        m.periods[0]
            .adaptations
            .iter()
            .filter(|a| a.contentType.as_deref() == Some("text"))
            .find(|a| a.lang.as_deref() == Some(lang))
            .expect("a text AdaptationSet for the language")
    }

    fn audio_set(m: &MPD) -> &AdaptationSet {
        m.periods[0]
            .adaptations
            .iter()
            .find(|a| a.contentType.as_deref() == Some("audio"))
            .expect("an audio AdaptationSet")
    }

    #[test]
    fn caption_text_gets_caption_role() {
        let m = mpd(
            &[],
            &[],
            &[text_track_role("eng", TextRole::Caption)],
            &[],
            0,
        );
        let set = text_set(&m, "eng");
        assert_eq!(set.Role.len(), 1);
        assert_eq!(set.Role[0].value.as_deref(), Some("caption"));
    }

    #[test]
    fn forced_text_gets_forced_subtitle_role() {
        let m = mpd(
            &[],
            &[],
            &[text_track_role("eng", TextRole::ForcedSubtitle)],
            &[],
            0,
        );
        let set = text_set(&m, "eng");
        assert_eq!(set.Role[0].value.as_deref(), Some("forced-subtitle"));
    }

    #[test]
    fn same_language_subtitle_and_caption_are_two_sets() {
        let texts = [
            text_track_role("eng", TextRole::Subtitle),
            text_track_role("eng", TextRole::Caption),
        ];
        let m = mpd(&[], &[], &texts, &[], 0);
        let count = m.periods[0]
            .adaptations
            .iter()
            .filter(|a| a.contentType.as_deref() == Some("text"))
            .count();
        assert_eq!(count, 2);
    }

    #[test]
    fn audio_description_gets_role_and_accessibility() {
        let m = mpd(
            &[],
            &[audio_track_role("eng", Some(AudioRole::Description))],
            &[],
            &[],
            0,
        );
        let set = audio_set(&m);
        assert_eq!(set.Role[0].value.as_deref(), Some("description"));
        assert_eq!(set.Accessibility.len(), 1);
        assert_eq!(
            set.Accessibility[0].schemeIdUri,
            "urn:tva:metadata:cs:AudioPurposeCS:2007"
        );
        assert_eq!(set.Accessibility[0].value.as_deref(), Some("1"));
    }

    #[test]
    fn audio_without_role_has_no_role_element() {
        let m = mpd(&[], &[audio_track_role("eng", None)], &[], &[], 0);
        let set = audio_set(&m);
        assert!(set.Role.is_empty());
        assert!(set.Accessibility.is_empty());
    }
}
