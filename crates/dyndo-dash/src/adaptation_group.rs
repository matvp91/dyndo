use dyndo_core::role::Role;
use dyndo_core::track::cmaf::{CmafKind, ResolvedCmafTrack};

pub(super) struct AdaptationGroup<'a> {
    key: String,
    content_type: &'static str,
    mime_type: &'static str,
    language: Option<String>,
    role: Option<Role>,
    members: Vec<&'a ResolvedCmafTrack>,
}

impl<'a> AdaptationGroup<'a> {
    fn new(key: String, track: &'a ResolvedCmafTrack) -> Self {
        let language = track.kind().language().map(ToString::to_string);
        let role = track.kind().role();

        Self {
            key,
            content_type: track.kind().content_type(),
            mime_type: track.kind().mime_type(),
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

    pub(super) fn members(&self) -> &[&'a ResolvedCmafTrack] {
        &self.members
    }
}

fn adaptation_set_key(track: &ResolvedCmafTrack) -> String {
    let codec = track.codec().rfc6381();
    let sample_entry = sample_entry(&codec);
    match track.kind() {
        CmafKind::Video(_) => {
            format!("video:{sample_entry}:{}", track.timescale())
        }
        CmafKind::Audio(audio) => format!(
            "audio:{sample_entry}:{}:{}:{}:{}:{}",
            track.timescale(),
            audio.language,
            audio.role.map_or("", |role| role.as_str()),
            audio.sample_rate,
            audio.channels
        ),
        CmafKind::Text(text) => format!(
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
