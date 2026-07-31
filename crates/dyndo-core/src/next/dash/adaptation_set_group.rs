use crate::next::role::Role;
use crate::next::segment_index::SegmentIndex;
use crate::next::track::Track;
use crate::next::track_metadata::Kind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AdaptationKey {
    Video {
        codec: String,
        timescale: u32,
        role: Option<Role>,
    },
    Audio {
        codec: String,
        timescale: u32,
        language: String,
        role: Option<Role>,
    },
    Text {
        codec: String,
        timescale: u32,
        language: String,
        role: Option<Role>,
    },
}

impl AdaptationKey {
    fn of(track: &Track, index: &SegmentIndex) -> Self {
        let codec = track
            .metadata
            .codec
            .as_deref()
            .unwrap_or_default()
            .to_string();
        let timescale = index.timescale;
        let role = track.metadata.role;
        match &track.metadata.kind {
            Kind::Video(_) => Self::Video {
                codec,
                timescale,
                role,
            },
            Kind::Audio(audio) => Self::Audio {
                codec,
                timescale,
                language: audio.language.clone(),
                role,
            },
            Kind::Text(text) => Self::Text {
                codec,
                timescale,
                language: text.language.clone(),
                role,
            },
        }
    }
}

pub(super) fn group<'asset, 'tracks>(
    tracks: &'tracks [(&'asset Track, SegmentIndex)],
) -> Vec<(
    AdaptationKey,
    Vec<&'tracks (&'asset Track, SegmentIndex)>,
)> {
    let mut groups: Vec<(AdaptationKey, Vec<&(&Track, SegmentIndex)>)> = Vec::new();
    for track @ (metadata, index) in tracks {
        let key = AdaptationKey::of(metadata, index);
        match groups.iter_mut().find(|(candidate, _)| *candidate == key) {
            Some((_, members)) => members.push(track),
            None => groups.push((key, vec![track])),
        }
    }
    groups
}
