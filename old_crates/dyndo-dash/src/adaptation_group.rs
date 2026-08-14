use dyndo_core::role::Role;
use dyndo_core::track::TrackType;
use dyndo_core::track::cmaf::{CmafMetadata, ResolvedCmafTrack};

pub(super) struct AdaptationGroup<'a> {
    key: String,
    track_type: TrackType,
    mime_type: &'static str,
    language: Option<String>,
    role: Option<Role>,
    members: Vec<&'a ResolvedCmafTrack>,
}

impl<'a> AdaptationGroup<'a> {
    fn new(key: String, track: &'a ResolvedCmafTrack) -> Self {
        let language = track.metadata().language().map(ToString::to_string);
        let role = track.metadata().role();

        Self {
            key,
            track_type: track.metadata().track_type(),
            mime_type: track.metadata().mime_type(),
            language,
            role,
            members: vec![track],
        }
    }

    pub(super) fn group(tracks: &'a [ResolvedCmafTrack]) -> Vec<Self> {
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

    pub(super) const fn track_type(&self) -> TrackType {
        self.track_type
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

    pub(super) fn members(&self) -> &[&'a ResolvedCmafTrack] {
        &self.members
    }
}

fn adaptation_set_key(track: &ResolvedCmafTrack) -> String {
    let codec = track.codec().rfc6381();
    let sample_entry = sample_entry(&codec);
    match track.metadata() {
        CmafMetadata::Video(_) => {
            format!("video:{sample_entry}:{}", track.timescale())
        }
        CmafMetadata::Audio(audio) => format!(
            "audio:{sample_entry}:{}:{}:{}:{}:{}",
            track.timescale(),
            audio.language,
            audio.role.map_or("", |role| role.as_str()),
            audio.sample_rate,
            audio.channels
        ),
        CmafMetadata::Text(text) => format!(
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
