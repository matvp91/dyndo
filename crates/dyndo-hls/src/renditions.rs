use dyndo_core::role::Role;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::served_segment::ServedSegment;
use dyndo_core::track::Track;
use dyndo_core::track_kind::TrackKind;
use language_tags::LanguageTag;
use m3u8_rs::{AlternativeMedia, AlternativeMediaType};

use crate::options::HlsOptions;
use crate::roles;

pub(crate) struct Renditions {
    pub(crate) has_audio: bool,
    pub(crate) has_subtitles: bool,
    pub(crate) maximum_bitrate: u64,
    pub(crate) average_bitrate: u64,
    codecs: Vec<String>,
}

impl Renditions {
    pub(crate) fn summarize(
        tracks: &[Track],
        segment_options: &SegmentOptions,
        hls_options: &HlsOptions,
    ) -> Self {
        let mut codecs = Vec::new();
        let mut has_audio = false;
        let mut has_subtitles = false;
        let mut audio_bitrates = (0, 0);
        let mut subtitle_bitrates = (0, 0);

        for track in tracks {
            let bitrates = match track.kind() {
                TrackKind::Video(_) => continue,
                TrackKind::Audio(_) => {
                    has_audio = true;
                    &mut audio_bitrates
                }
                TrackKind::Text(_) => {
                    has_subtitles = true;
                    &mut subtitle_bitrates
                }
            };
            if hls_options.wvtt || !matches!(track.kind(), TrackKind::Text(_)) {
                push_unique(&mut codecs, track.codec().rfc6381());
            }
            let segments = ServedSegment::group(
                track.segments(),
                segment_options.min_length,
                &segment_options.boundaries,
            );
            bitrates.0 = bitrates.0.max(ServedSegment::maximum_bitrate(&segments));
            bitrates.1 = bitrates.1.max(ServedSegment::average_bitrate(&segments));
        }

        Self {
            has_audio,
            has_subtitles,
            codecs,
            maximum_bitrate: audio_bitrates.0.saturating_add(subtitle_bitrates.0),
            average_bitrate: audio_bitrates.1.saturating_add(subtitle_bitrates.1),
        }
    }

    pub(crate) fn codecs_for(&self, track: &Track) -> Vec<String> {
        let mut codecs = Vec::with_capacity(self.codecs.len() + 1);
        push_unique(&mut codecs, track.codec().rfc6381());
        for codec in &self.codecs {
            push_unique(&mut codecs, codec.clone());
        }
        codecs
    }

    pub(crate) fn media_entries(tracks: &[Track]) -> Vec<AlternativeMedia> {
        let default_audio_id = default_audio_id(tracks);
        tracks
            .iter()
            .filter_map(|track| build_media_entry(tracks, track, default_audio_id))
            .collect()
    }
}

fn build_media_entry(
    tracks: &[Track],
    track: &Track,
    default_audio_id: Option<&str>,
) -> Option<AlternativeMedia> {
    let (media_type, group_id, language, role, channels) = match track.kind() {
        TrackKind::Video(_) => return None,
        TrackKind::Audio(audio) => (
            AlternativeMediaType::Audio,
            "audio",
            &audio.language,
            audio.role,
            Some(audio.channels.to_string()),
        ),
        TrackKind::Text(text) => (
            AlternativeMediaType::Subtitles,
            "subtitles",
            &text.language,
            text.role,
            None,
        ),
    };
    let default = default_audio_id == Some(track.id());
    Some(AlternativeMedia {
        media_type,
        uri: Some(format!("{}.m3u8", crate::media_resource_name(track))),
        group_id: group_id.to_string(),
        language: Some(language.to_string()),
        assoc_language: None,
        name: roles::name(language, role),
        default,
        autoselect: default || selection_tuple_is_unique(tracks, track),
        forced: roles::is_forced(role),
        instream_id: None,
        characteristics: roles::characteristics(role).map(str::to_string),
        channels,
        other_attributes: None,
    })
}

fn default_audio_id(tracks: &[Track]) -> Option<&str> {
    tracks
        .iter()
        .find(|track| {
            matches!(track.kind(), TrackKind::Audio(audio) if audio.role == Some(Role::Main))
        })
        .or_else(|| {
            tracks.iter().find(
                |track| matches!(track.kind(), TrackKind::Audio(audio) if audio.role.is_none()),
            )
        })
        .map(Track::id)
}

fn selection_tuple_is_unique(tracks: &[Track], track: &Track) -> bool {
    let Some((is_audio, language, role)) = selection_tuple(track) else {
        return false;
    };
    tracks
        .iter()
        .filter_map(selection_tuple)
        .filter(|candidate| {
            candidate.0 == is_audio
                && candidate.1 == language
                && roles::is_forced(candidate.2) == roles::is_forced(role)
                && roles::characteristics(candidate.2) == roles::characteristics(role)
        })
        .count()
        == 1
}

fn selection_tuple(track: &Track) -> Option<(bool, &LanguageTag, Option<Role>)> {
    match track.kind() {
        TrackKind::Video(_) => None,
        TrackKind::Audio(audio) => Some((true, &audio.language, audio.role)),
        TrackKind::Text(text) => Some((false, &text.language, text.role)),
    }
}

fn push_unique(codecs: &mut Vec<String>, codec: String) {
    if !codecs.contains(&codec) {
        codecs.push(codec);
    }
}
