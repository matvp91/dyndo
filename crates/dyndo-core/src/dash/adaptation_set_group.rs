//! Groups an asset's tracks into DASH adaptation sets: [`AdaptationKey`]
//! names the fields that must be uniform within one set, [`group`] buckets
//! the advertisable tracks by that key. Rendering the groups into
//! `AdaptationSet` XML stays in `build`.

use crate::codec;
use crate::header::Header;
use crate::metadata::Metadata;
use crate::role::{AudioRole, TextRole};
use crate::track::Track;

/// What places two tracks in the same `AdaptationSet`: the fields the spec
/// requires to be uniform across a set's `Representation`s. Every set holds
/// one media type, one decoder — the sample-entry codingname; DASH-IF IOP
/// v4.3 §6.2.5.1 forbids mixing e.g. `avc1` and `hev1` — and one
/// `@timescale` (§3.2.10.2). Audio and text alternatives further split by
/// language and role. Color parameters (primaries/transfer/matrix, also
/// §6.2.5.1) belong here too once dyndo probes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AdaptationKey {
    /// A video set: bitrates and resolutions of the one video content.
    Video {
        /// Sample-entry codingname (e.g. `"avc1"`).
        sample_entry: String,
        /// Track timescale, in units per second.
        timescale: u32,
    },
    /// An audio set: bitrates of one language + role alternative.
    Audio {
        /// Sample-entry codingname (e.g. `"mp4a"`).
        sample_entry: String,
        /// Track timescale, in units per second.
        timescale: u32,
        /// ISO-639-2 language code (`"und"` when undeclared; the metadata
        /// layer guarantees it is filled).
        language: String,
        /// The track's declared purpose.
        role: Option<AudioRole>,
    },
    /// A text set: one language + role alternative.
    Text {
        /// Sample-entry codingname (e.g. `"wvtt"`).
        sample_entry: String,
        /// Track timescale, in units per second.
        timescale: u32,
        /// ISO-639-2 language code (`"und"` when unspecified).
        language: String,
        /// The track's declared purpose.
        role: Option<TextRole>,
    },
}

impl AdaptationKey {
    /// Where `track` fits in the MPD, or `None` when it is not advertised:
    /// a raw (non-CMAF) file has no fragment timeline to put in a
    /// `SegmentTemplate`.
    fn of(track: &Track) -> Option<AdaptationKey> {
        let h = match track.header() {
            Header::Raw(_) => return None,
            Header::Cmaf(h) => h,
        };
        let sample_entry = codec::rfc6381_sample_entry(&h.codec).to_string();
        Some(match &track.metadata {
            Metadata::Video(_) => AdaptationKey::Video {
                sample_entry,
                timescale: h.timescale,
            },
            Metadata::Audio(a) => AdaptationKey::Audio {
                sample_entry,
                timescale: h.timescale,
                language: a.language.clone(),
                role: a.role,
            },
            Metadata::Text(t) => AdaptationKey::Text {
                sample_entry,
                timescale: h.timescale,
                language: t.language.clone(),
                role: t.role,
            },
        })
    }
}

/// Group `tracks` into adaptation sets: one group per distinct
/// [`AdaptationKey`], keys and members both in first-seen track order.
/// Tracks with no key (raw files) are left out.
pub(super) fn group(tracks: &[Track]) -> Vec<(AdaptationKey, Vec<&Track>)> {
    let mut groups: Vec<(AdaptationKey, Vec<&Track>)> = Vec::new();
    for track in tracks {
        let Some(key) = AdaptationKey::of(track) else {
            continue;
        };
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, members)) => members.push(track),
            None => groups.push((key, vec![track])),
        }
    }
    groups
}
