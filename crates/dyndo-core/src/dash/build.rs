//! Pure `(id, CmafHeader)` -> `dash_mpd::MPD` construction (no I/O).

use std::time::Duration;

use dash_mpd::{
    AdaptationSet, AudioChannelConfiguration, Period, Representation, SegmentTemplate,
    SegmentTimeline, MPD,
};

use crate::cmaf::CmafHeader;
use crate::dash::timeline::build_timeline;

const INIT_TEMPLATE: &str = "$RepresentationID$/init.mp4";
const MEDIA_TEMPLATE: &str = "$RepresentationID$/$Time$.m4s";

enum Kind {
    Video,
    Audio,
}

fn header_kind(h: &CmafHeader) -> Kind {
    match h {
        CmafHeader::Video(_) => Kind::Video,
        CmafHeader::Audio(_) => Kind::Audio,
    }
}

fn fourcc(h: &CmafHeader) -> &'static str {
    match h {
        CmafHeader::Video(v) => v.codec.fourcc(),
        CmafHeader::Audio(a) => a.codec.fourcc(),
    }
}

fn language(h: &CmafHeader) -> Option<&str> {
    match h {
        CmafHeader::Audio(a) => a.language.as_deref(),
        CmafHeader::Video(_) => None,
    }
}

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
            let (ts, segs) = match h {
                CmafHeader::Video(v) => (v.timescale, &v.segments),
                CmafHeader::Audio(a) => (a.timescale, &a.segments),
            };
            segs.iter().map(move |s| secs(s.duration, ts))
        })
        .fold(0.0_f64, f64::max)
}

fn media_presentation_duration_secs(tracks: &[(String, CmafHeader)]) -> f64 {
    tracks
        .iter()
        .map(|(_, h)| match h {
            CmafHeader::Video(v) => secs(v.duration, v.timescale),
            CmafHeader::Audio(a) => secs(a.duration, a.timescale),
        })
        .fold(0.0_f64, f64::max)
}

fn segment_template(
    timescale: u32,
    pto: u64,
    segments: &[crate::cmaf::Segment],
    first_t: u64,
) -> SegmentTemplate {
    SegmentTemplate {
        timescale: Some(timescale as u64),
        presentationTimeOffset: Some(pto),
        initialization: Some(INIT_TEMPLATE.to_string()),
        media: Some(MEDIA_TEMPLATE.to_string()),
        SegmentTimeline: Some(SegmentTimeline {
            segments: build_timeline(segments, first_t),
        }),
        ..Default::default()
    }
}

fn representation(id: &str, h: &CmafHeader) -> Representation {
    match h {
        CmafHeader::Video(v) => Representation {
            id: Some(id.to_string()),
            bandwidth: Some(v.bandwidth as u64),
            codecs: Some(v.codec.rfc6381()),
            width: Some(v.width as u64),
            height: Some(v.height as u64),
            frameRate: Some(frame_rate_str(v.frame_rate)),
            SegmentTemplate: Some(segment_template(
                v.timescale,
                v.earliest_presentation_time,
                &v.segments,
                v.earliest_presentation_time,
            )),
            ..Default::default()
        },
        CmafHeader::Audio(a) => Representation {
            id: Some(id.to_string()),
            bandwidth: Some(a.bandwidth as u64),
            codecs: Some(a.codec.rfc6381()),
            audioSamplingRate: Some(a.sample_rate.to_string()),
            AudioChannelConfiguration: vec![AudioChannelConfiguration {
                schemeIdUri: "urn:mpeg:dash:23003:3:audio_channel_configuration:2011".to_string(),
                value: Some(a.channels.to_string()),
                ..Default::default()
            }],
            SegmentTemplate: Some(segment_template(
                a.timescale,
                a.earliest_presentation_time,
                &a.segments,
                a.earliest_presentation_time,
            )),
            ..Default::default()
        },
    }
}

/// `(is_video, fourcc, language)` grouping key for `AdaptationSet` assignment.
type GroupKey = (bool, &'static str, Option<String>);

/// Build a static VOD `MPD` from `(representation_id, CmafHeader)` tracks. Pure:
/// no I/O. Tracks are grouped into one `AdaptationSet` per `(is_video, fourcc,
/// language)` key, videos before audios, each track becoming one `Representation`
/// with its own `SegmentTemplate`.
// used by generate_mpd (Task 9)
#[allow(dead_code)]
pub(crate) fn build_mpd(tracks: &[(String, CmafHeader)]) -> MPD {
    // Group by (is_video, fourcc, language), preserving first-seen order per group,
    // videos before audios.
    let mut groups: Vec<(GroupKey, Vec<usize>)> = Vec::new();
    for (i, (_, h)) in tracks.iter().enumerate() {
        let key = (
            matches!(header_kind(h), Kind::Video),
            fourcc(h),
            language(h).map(str::to_string),
        );
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, idxs)) => idxs.push(i),
            None => groups.push((key, vec![i])),
        }
    }
    groups.sort_by(|(a, _), (b, _)| b.0.cmp(&a.0)); // videos (true) first

    let adaptations = groups
        .iter()
        .enumerate()
        .map(|(set_id, ((is_video, _fourcc, lang), idxs))| {
            let representations = idxs
                .iter()
                .map(|&i| {
                    let (id, h) = &tracks[i];
                    representation(id, h)
                })
                .collect();
            AdaptationSet {
                id: Some(set_id.to_string()),
                contentType: Some(if *is_video { "video" } else { "audio" }.to_string()),
                mimeType: Some(if *is_video { "video/mp4" } else { "audio/mp4" }.to_string()),
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
        xmlns: Some("urn:mpeg:dash:schema:mpd:2011".to_string()),
        mpdtype: Some("static".to_string()),
        profiles: Some("urn:mpeg:dash:profile:isoff-live:2011".to_string()),
        minBufferTime: Some(Duration::from_secs_f64(max_segment_duration_secs(tracks))),
        mediaPresentationDuration: Some(Duration::from_secs_f64(
            media_presentation_duration_secs(tracks),
        )),
        periods: vec![period],
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmaf::{AudioCodec, AudioCmafHeader, ByteRange, Segment, VideoCmafHeader, VideoCodec};

    fn video(id: &str) -> (String, CmafHeader) {
        (
            id.to_string(),
            CmafHeader::Video(VideoCmafHeader {
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
                codec: VideoCodec::Avc {
                    profile: 0x64,
                    constraints: 0,
                    level: 0x28
                },
                width: 1920,
                height: 1080,
                frame_rate: (25, 1),
            }),
        )
    }

    fn audio(id: &str, lang: Option<&str>) -> (String, CmafHeader) {
        (
            id.to_string(),
            CmafHeader::Audio(AudioCmafHeader {
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
                codec: AudioCodec::Aac {
                    audio_object_type: 2
                },
                sample_rate: 48000,
                channels: 2,
                language: lang.map(str::to_string),
            }),
        )
    }

    #[test]
    fn groups_video_and_audio_into_separate_adaptation_sets() {
        let mpd = build_mpd(&[video("v0"), audio("a0", Some("nld"))]);
        let period = &mpd.periods[0];
        assert_eq!(period.adaptations.len(), 2);
        assert_eq!(mpd.mpdtype.as_deref(), Some("static"));
    }

    #[test]
    fn video_representation_carries_codec_dims_and_framerate() {
        let mpd = build_mpd(&[video("v0")]);
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
        let mpd = build_mpd(&[video("v0"), video("v1")]);
        assert_eq!(mpd.periods[0].adaptations.len(), 1);
        assert_eq!(mpd.periods[0].adaptations[0].representations.len(), 2);
    }
}
