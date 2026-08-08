use dyndo_core::asset_descriptor::TrackKind;
use dyndo_core::role::Role;
use dyndo_core::segment::{self, Segment, SegmentOptions};
use dyndo_core::track::Track;

pub(super) type Member<'a> = &'a Track;

pub(super) struct AdaptationSetGroup<'a> {
    key: String,
    content_type: &'static str,
    mime_type: &'static str,
    language: Option<String>,
    role: Option<Role>,
    members: Vec<Member<'a>>,
}

impl<'a> AdaptationSetGroup<'a> {
    fn new(key: String, track: &'a Track) -> Self {
        let language = match track.kind() {
            TrackKind::Video(_) => None,
            TrackKind::Audio(audio) => Some(audio.language.to_string()),
            TrackKind::Text(text) => Some(text.language.to_string()),
        };
        let role = match track.kind() {
            TrackKind::Video(_) => None,
            TrackKind::Audio(audio) => audio.role,
            TrackKind::Text(text) => text.role,
        };

        Self {
            key,
            content_type: track.kind().content_type(),
            mime_type: track.kind().mime_type(),
            language,
            role,
            members: vec![track],
        }
    }

    pub(super) fn content_type(&self) -> &'static str {
        self.content_type
    }

    pub(super) fn mime_type(&self) -> &'static str {
        self.mime_type
    }

    pub(super) fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    pub(super) fn role(&self) -> Option<Role> {
        self.role
    }

    pub(super) fn members(&self) -> &[Member<'a>] {
        &self.members
    }

    /// Whether every member cuts its segments at the same times, which is what lets
    /// one timeline stand for all of them.
    ///
    /// Comparing the times themselves covers where the tracks begin and how they are
    /// cut in one go: two tracks with matching durations but different earliest
    /// presentation times never line up, and neither do two whose last segments differ
    /// in length.
    pub(super) fn is_segment_aligned(&self, options: &SegmentOptions) -> bool {
        let Some(reference) = self.members.first() else {
            return true;
        };
        let reference_segments = segment::segments(reference, options);

        self.members.iter().skip(1).all(|candidate| {
            segment::segments(candidate, options)
                .iter()
                .map(Segment::raw_range)
                .eq(reference_segments.iter().map(Segment::raw_range))
        })
    }
}

pub(super) fn group(tracks: &[Track]) -> Vec<AdaptationSetGroup<'_>> {
    let mut groups: Vec<AdaptationSetGroup<'_>> = Vec::new();

    for track in tracks {
        let key = adaptation_set_key(track);
        if let Some(group) = groups.iter_mut().find(|group| group.key == key) {
            group.members.push(track);
        } else {
            groups.push(AdaptationSetGroup::new(key, track));
        }
    }

    groups
}

fn adaptation_set_key(track: &Track) -> String {
    let sample_entry = sample_entry(track.codec());
    match track.kind() {
        TrackKind::Video(_) => {
            format!("video:{sample_entry}:{}", track.timescale())
        }
        TrackKind::Audio(audio) => format!(
            "audio:{sample_entry}:{}:{}:{}:{}:{}",
            track.timescale(),
            audio.language,
            audio.role.map_or("", |role| role.as_str()),
            audio.sample_rate,
            audio.channels
        ),
        TrackKind::Text(text) => format!(
            "text:{sample_entry}:{}:{}:{}",
            track.timescale(),
            text.language,
            text.role.map_or("", |role| role.as_str())
        ),
    }
}

fn sample_entry(codec: &str) -> &str {
    codec.split_once('.').map_or(codec, |(entry, _)| entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_entry_removes_codec_parameters() {
        assert_eq!(sample_entry("avc1.640028"), "avc1");
    }

    #[test]
    fn sample_entry_preserves_unparameterized_codec() {
        assert_eq!(sample_entry("ac-3"), "ac-3");
    }
}
