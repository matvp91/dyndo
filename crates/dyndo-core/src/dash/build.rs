//! Pure `(id, CmafHeader)` -> `dash_mpd::MPD` construction (no I/O).

use std::time::Duration;

use dash_mpd::{
    AdaptationSet, AudioChannelConfiguration, Period, Representation, SegmentTemplate,
    SegmentTimeline, MPD,
};

use crate::cmaf::{CmafHeader, Stream};
use crate::dash::timeline::build_timeline;
use crate::util::group_by_key;

const INIT_TEMPLATE: &str = "$RepresentationID$/init.mp4";
const MEDIA_TEMPLATE: &str = "$RepresentationID$/$Time$.m4s";
const MPD_XMLNS: &str = "urn:mpeg:dash:schema:mpd:2011";
const MPD_PROFILE: &str = "urn:mpeg:dash:profile:isoff-live:2011";
const AUDIO_CHANNEL_CONFIG_SCHEME: &str = "urn:mpeg:dash:23003:3:audio_channel_configuration:2011";

fn secs(units: u64, timescale: u32) -> f64 {
    if timescale == 0 {
        0.0
    } else {
        units as f64 / timescale as f64
    }
}

fn frame_rate_str(fr: (u32, u32)) -> String {
    if fr.1 == 1 {
        fr.0.to_string()
    } else {
        format!("{}/{}", fr.0, fr.1)
    }
}

fn max_segment_duration_secs(tracks: &[(String, CmafHeader)]) -> f64 {
    tracks
        .iter()
        .flat_map(|(_, h)| {
            h.segments
                .iter()
                .map(move |s| secs(s.duration, h.timescale))
        })
        .fold(0.0_f64, f64::max)
}

fn media_presentation_duration_secs(tracks: &[(String, CmafHeader)]) -> f64 {
    tracks
        .iter()
        .map(|(_, h)| secs(h.duration, h.timescale))
        .fold(0.0_f64, f64::max)
}

/// In static VOD the SegmentTimeline's first `t` equals the presentation time
/// offset (both are the earliest presentation time); a live/rolling window would
/// need them tracked separately.
fn segment_template(
    timescale: u32,
    pto: u64,
    segments: &[crate::cmaf::Segment],
) -> SegmentTemplate {
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

fn representation(id: &str, h: &CmafHeader) -> Representation {
    let base = Representation {
        id: Some(id.to_string()),
        bandwidth: Some(h.bandwidth as u64),
        codecs: Some(h.stream.rfc6381()),
        SegmentTemplate: Some(segment_template(
            h.timescale,
            h.earliest_presentation_time,
            &h.segments,
        )),
        ..Default::default()
    };
    match &h.stream {
        Stream::Video(v) => Representation {
            width: Some(v.width as u64),
            height: Some(v.height as u64),
            frameRate: (v.frame_rate.0 != 0).then(|| frame_rate_str(v.frame_rate)),
            ..base
        },
        Stream::Audio(a) => Representation {
            audioSamplingRate: Some(a.sample_rate.to_string()),
            AudioChannelConfiguration: vec![AudioChannelConfiguration {
                schemeIdUri: AUDIO_CHANNEL_CONFIG_SCHEME.to_string(),
                value: Some(a.channels.to_string()),
                ..Default::default()
            }],
            ..base
        },
    }
}

/// Build the raw static VOD `MPD` from `(representation_id, CmafHeader)` tracks.
/// Pure: no I/O, no compaction. Tracks are grouped into one `AdaptationSet` per
/// `(fourcc, language)` key, each track becoming one `Representation` with its own
/// `SegmentTemplate`. `AdaptationSet` order follows the order groups first appear in
/// `tracks`.
fn mpd(tracks: &[(String, CmafHeader)]) -> MPD {
    // Group by (fourcc, language), preserving the order each group's first track
    // appears in the input. `fourcc` alone already separates video from audio, so
    // the media kind need not be part of the key.
    let groups = group_by_key(tracks, |(_, h)| {
        (h.stream.fourcc(), h.stream.language().map(str::to_string))
    });

    let adaptations = groups
        .iter()
        .enumerate()
        .map(|(set_id, ((_fourcc, lang), idxs))| {
            let representations = idxs
                .iter()
                .map(|&i| {
                    let (id, h) = &tracks[i];
                    representation(id, h)
                })
                .collect();
            // Every track in a group shares a media kind (they share a fourcc), so a
            // representative track determines the set's content and mime type.
            let (content_type, mime_type) = match &tracks[idxs[0]].1.stream {
                Stream::Video(_) => ("video", "video/mp4"),
                Stream::Audio(_) => ("audio", "audio/mp4"),
            };
            AdaptationSet {
                id: Some(set_id.to_string()),
                contentType: Some(content_type.to_string()),
                mimeType: Some(mime_type.to_string()),
                lang: lang.clone(),
                segmentAlignment: Some(true),
                startWithSAP: Some(1),
                representations,
                ..Default::default()
            }
        })
        .collect();

    let period = Period {
        id: Some("0".to_string()),
        start: Some(Duration::ZERO),
        adaptations,
        ..Default::default()
    };

    MPD {
        xmlns: Some(MPD_XMLNS.to_string()),
        mpdtype: Some("static".to_string()),
        profiles: Some(MPD_PROFILE.to_string()),
        minBufferTime: Some(Duration::from_secs_f64(max_segment_duration_secs(tracks))),
        mediaPresentationDuration: Some(Duration::from_secs_f64(media_presentation_duration_secs(
            tracks,
        ))),
        periods: vec![period],
        ..Default::default()
    }
}

/// Assemble the final `MPD`: build it from `tracks`, then — when `compact` is set —
/// hoist `SegmentTemplate` content shared across each `AdaptationSet`'s
/// `Representation`s up to the set level (see [`super::compact`]). Compaction is a
/// pure size optimization that preserves the effective per-Representation template
/// under DASH multi-level inheritance (ISO/IEC 23009-1 §5.3.9.1).
pub(crate) fn build_mpd(tracks: &[(String, CmafHeader)], compact: bool) -> MPD {
    let mut m = mpd(tracks);
    if compact {
        super::compact::compact(&mut m);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmaf::{AudioCodec, AudioStream, ByteRange, Segment, VideoCodec, VideoStream};

    fn video(id: &str) -> (String, CmafHeader) {
        (
            id.to_string(),
            CmafHeader {
                timescale: 90000,
                duration: 900000,
                bandwidth: 4_800_000,
                earliest_presentation_time: 0,
                init_range: ByteRange { start: 0, end: 700 },
                segments: vec![
                    Segment {
                        offset: 0,
                        size: 0,
                        duration: 180000
                    };
                    5
                ],
                stream: Stream::Video(VideoStream {
                    codec: VideoCodec::Avc {
                        profile: 0x64,
                        constraints: 0,
                        level: 0x28,
                    },
                    width: 1920,
                    height: 1080,
                    frame_rate: (25, 1),
                }),
            },
        )
    }

    fn audio(id: &str, lang: Option<&str>) -> (String, CmafHeader) {
        (
            id.to_string(),
            CmafHeader {
                timescale: 48000,
                duration: 480000,
                bandwidth: 128_000,
                earliest_presentation_time: 0,
                init_range: ByteRange { start: 0, end: 600 },
                segments: vec![
                    Segment {
                        offset: 0,
                        size: 0,
                        duration: 96000
                    };
                    5
                ],
                stream: Stream::Audio(AudioStream {
                    codec: AudioCodec::Aac {
                        audio_object_type: 2,
                    },
                    sample_rate: 48000,
                    channels: 2,
                    language: lang.map(str::to_string),
                }),
            },
        )
    }

    #[test]
    fn groups_video_and_audio_into_separate_adaptation_sets() {
        let mpd = mpd(&[video("v0"), audio("a0", Some("nld"))]);
        let period = &mpd.periods[0];
        assert_eq!(period.adaptations.len(), 2);
        assert_eq!(mpd.mpdtype.as_deref(), Some("static"));
    }

    #[test]
    fn adaptation_sets_follow_input_track_order() {
        // Audio listed before video in the input -> audio AdaptationSet comes first.
        let mpd = mpd(&[audio("a0", Some("nld")), video("v0")]);
        let sets = &mpd.periods[0].adaptations;
        assert_eq!(sets[0].contentType.as_deref(), Some("audio"));
        assert_eq!(sets[1].contentType.as_deref(), Some("video"));
    }

    #[test]
    fn video_representation_carries_codec_dims_and_framerate() {
        let mpd = mpd(&[video("v0")]);
        let rep = &mpd.periods[0].adaptations[0].representations[0];
        assert_eq!(rep.id.as_deref(), Some("v0"));
        assert_eq!(rep.codecs.as_deref(), Some("avc1.640028"));
        assert_eq!(rep.width, Some(1920));
        assert_eq!(rep.height, Some(1080));
        assert_eq!(rep.frameRate.as_deref(), Some("25"));
        let st = rep.SegmentTemplate.as_ref().unwrap();
        assert_eq!(st.timescale, Some(90000));
        assert_eq!(st.presentationTimeOffset, Some(0));
        assert_eq!(st.media.as_deref(), Some("$RepresentationID$/$Time$.m4s"));
        assert_eq!(st.SegmentTimeline.as_ref().unwrap().segments[0].r, Some(4));
    }

    #[test]
    fn same_codec_videos_share_one_adaptation_set() {
        let mpd = mpd(&[video("v0"), video("v1")]);
        assert_eq!(mpd.periods[0].adaptations.len(), 1);
        assert_eq!(mpd.periods[0].adaptations[0].representations.len(), 2);
    }

    #[test]
    fn build_mpd_compacts_only_when_requested() {
        let tracks = [video("v0"), video("v1")];

        // compact = false: each Representation keeps its own SegmentTemplate.
        let plain = build_mpd(&tracks, false);
        let set = &plain.periods[0].adaptations[0];
        assert!(set.SegmentTemplate.is_none());
        assert!(set
            .representations
            .iter()
            .all(|r| r.SegmentTemplate.is_some()));

        // compact = true: the shared template is hoisted to the AdaptationSet.
        let compacted = build_mpd(&tracks, true);
        let set = &compacted.periods[0].adaptations[0];
        assert!(set.SegmentTemplate.is_some());
        assert!(set
            .representations
            .iter()
            .all(|r| r.SegmentTemplate.is_none()));
    }
}
