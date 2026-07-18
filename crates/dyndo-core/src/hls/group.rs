//! Groups an asset's tracks for the HLS multivariant playlist: which tracks
//! are advertised — CMAF video and audio; text and raw (non-CMAF) tracks are
//! not — and which audio renditions share one `EXT-X-MEDIA` `GROUP-ID`.
//! Rendering the groups into playlist tags stays in `build`.

use crate::asset::Asset;
use crate::codec::rfc6381_sample_entry;
use crate::header::Header;
use crate::metadata::{AudioMetadata, Metadata, VideoMetadata};
use crate::track::Track;

/// Audio tracks sharing one sample-entry codingname → one `EXT-X-MEDIA`
/// `GROUP-ID`: a variant's `CODECS` covers the renditions of the group it
/// references, so a group must hold a single decoder's formats.
pub(super) struct AudioGroup<'a> {
    /// `GROUP-ID` = the sample-entry codingname (`"mp4a"`, `"ec-3"`, …).
    pub(super) id: String,
    /// A representative RFC 6381 string for the group's `CODECS` contribution.
    pub(super) codec: String,
    /// The highest-bandwidth member's bandwidth, added to a variant's `BANDWIDTH`.
    pub(super) max_bandwidth: u32,
    /// The group's renditions in first-seen order; the default is chosen by
    /// `audio_media` in `build` (first `main`-role rendition, else the first).
    pub(super) tracks: Vec<(&'a Track, &'a AudioMetadata)>,
}

/// The asset's advertisable video tracks, in descriptor order.
pub(super) fn videos(asset: &Asset) -> Vec<(&Track, &VideoMetadata)> {
    asset
        .tracks
        .iter()
        .filter(|t| matches!(t.header(), Header::Cmaf(_)))
        .filter_map(|t| match &t.metadata {
            Metadata::Video(v) => Some((t, v)),
            _ => None,
        })
        .collect()
}

/// The asset's advertisable audio tracks, in descriptor order.
pub(super) fn audios(asset: &Asset) -> Vec<(&Track, &AudioMetadata)> {
    asset
        .tracks
        .iter()
        .filter(|t| matches!(t.header(), Header::Cmaf(_)))
        .filter_map(|t| match &t.metadata {
            Metadata::Audio(a) => Some((t, a)),
            _ => None,
        })
        .collect()
}

/// Group audio tracks by sample-entry codingname, preserving first-seen
/// order.
pub(super) fn audio_group<'a>(audios: &[(&'a Track, &'a AudioMetadata)]) -> Vec<AudioGroup<'a>> {
    let mut groups: Vec<AudioGroup> = Vec::new();
    for &(t, a) in audios {
        let h = t.cmaf();
        let sample_entry = rfc6381_sample_entry(&h.codec);
        match groups.iter_mut().find(|g| g.id == sample_entry) {
            Some(g) => {
                g.max_bandwidth = g.max_bandwidth.max(h.bandwidth());
                g.tracks.push((t, a));
            }
            None => groups.push(AudioGroup {
                id: sample_entry.to_string(),
                codec: h.codec.clone(),
                max_bandwidth: h.bandwidth(),
                tracks: vec![(t, a)],
            }),
        }
    }
    groups
}
