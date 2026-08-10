use dyndo_core::role::Role;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::served_segment::ServedSegment;
use dyndo_core::track::Track;
use dyndo_core::track_kind::TrackKind;

pub(super) struct AdaptationGroup<'a> {
    key: String,
    content_type: &'static str,
    mime_type: &'static str,
    language: Option<String>,
    role: Option<Role>,
    members: Vec<&'a Track>,
}

impl<'a> AdaptationGroup<'a> {
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

    pub(super) fn group(tracks: &'a [Track]) -> Vec<Self> {
        let mut groups: Vec<Self> = Vec::new();

        for track in tracks {
            let key = adaptation_set_key(track);
            if let Some(group) = groups.iter_mut().find(|group| group.key == key) {
                group.members.push(track);
            } else {
                groups.push(Self::new(key, track));
            }
        }

        groups
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

    pub(super) fn members(&self) -> &[&'a Track] {
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
        let reference_segments = served_segments(reference, options);

        self.members.iter().skip(1).all(|candidate| {
            served_segments(candidate, options)
                .iter()
                .map(segment_times)
                .eq(reference_segments.iter().map(segment_times))
        })
    }
}

fn served_segments<'a>(track: &'a Track, options: &SegmentOptions) -> Vec<ServedSegment<'a>> {
    ServedSegment::group(track.segments(), options.min_length, &options.boundaries)
}

fn segment_times(segment: &ServedSegment<'_>) -> (u64, u64) {
    (segment.unscaled_start_time(), segment.unscaled_end_time())
}

fn adaptation_set_key(track: &Track) -> String {
    let codec = track.codec().rfc6381();
    let sample_entry = sample_entry(&codec);
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
