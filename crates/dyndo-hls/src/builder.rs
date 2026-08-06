//! HLS playlist construction from dyndo assets.

use std::borrow::Cow;
use std::collections::HashSet;
use std::time::Duration;

use dyndo_core::asset_descriptor::{AssetDescriptor, TrackDescriptor, TrackKind};
use dyndo_core::segment::SegmentOptions;
use dyndo_core::track::{Track, TrackError, average_bitrate, max_bitrate};
use hls_m3u8::tags::{ExtXMap, ExtXMedia, VariantStream};
use hls_m3u8::types::{Channels, ClosedCaptions, MediaType, PlaylistType, StreamData, UFloat};
use hls_m3u8::{MasterPlaylist, MediaPlaylist, MediaSegment};
use language_tags::LanguageTag;
use opendal::Operator;

use crate::options::HlsOptions;
use crate::roles;

const AUDIO_GROUP_ID: &str = "audio";
const SUBTITLES_GROUP_ID: &str = "subtitles";

#[derive(Debug, thiserror::Error)]
pub enum HlsError {
    #[error(transparent)]
    Track(#[from] TrackError),
    #[error(transparent)]
    Playlist(#[from] hls_m3u8::Error),
    #[error("invalid video frame rate: {0}")]
    InvalidFrameRate(String),
    #[error("duplicate rendition name: {0}")]
    DuplicateRenditionName(String),
    #[error("segment start time overflow for track {0}")]
    SegmentTimeOverflow(String),
}

/// Generates an HLS multivariant playlist containing the asset's video tracks.
///
/// # Errors
///
/// Returns an error when a track cannot be probed or the resulting playlist is
/// rejected by `hls_m3u8`.
pub async fn generate_master_playlist(
    op: &Operator,
    asset: &AssetDescriptor,
    _hls_options: &HlsOptions,
) -> Result<MasterPlaylist<'static>, HlsError> {
    ensure_unique_rendition_names(asset)?;
    let tracks = Track::probe_all(op, asset).await?;
    build_master_playlist(asset, &tracks, &asset.segment_options)
}

/// Generates the static HLS media playlist for one asset track.
///
/// # Errors
///
/// Returns an error when the track cannot be probed, a segment timestamp
/// overflows, or `hls_m3u8` rejects the resulting playlist.
pub async fn generate_media_playlist(
    op: &Operator,
    asset: &AssetDescriptor,
    descriptor: &TrackDescriptor,
    _hls_options: &HlsOptions,
) -> Result<MediaPlaylist<'static>, HlsError> {
    let path = asset.track_path(descriptor);
    let segment_options = &asset.segment_options;
    let track = Track::probe(op, &path, Some(descriptor.kind.clone()), segment_options).await?;
    build_media_playlist(descriptor, &track, segment_options)
}

/// Serializes a media playlist with `EXTINF` durations rounded to three decimals.
pub fn serialize_media_playlist(playlist: &MediaPlaylist<'_>) -> String {
    let serialized = playlist.to_string();
    let mut output = String::with_capacity(serialized.len());
    for line in serialized.split_inclusive('\n') {
        let Some(value) = line.strip_prefix("#EXTINF:") else {
            output.push_str(line);
            continue;
        };
        let Some((duration, suffix)) = value.split_once(',') else {
            output.push_str(line);
            continue;
        };
        let Ok(duration) = duration.parse::<f64>() else {
            output.push_str(line);
            continue;
        };
        output.push_str(&format!("#EXTINF:{duration:.3},{suffix}"));
    }
    output
}

fn build_media_playlist(
    descriptor: &TrackDescriptor,
    track: &Track,
    segment_options: &SegmentOptions,
) -> Result<MediaPlaylist<'static>, HlsError> {
    let mut start_time = track.earliest_presentation_time();
    let segments = track.segments(segment_options);
    let target_duration = segments
        .iter()
        .map(|segment| rounded_duration_seconds(segment.raw_duration(), track.timescale()))
        .max()
        .unwrap_or(0);
    let segments = segments
        .into_iter()
        .enumerate()
        .map(|(index, segment)| {
            let duration = media_duration(segment.raw_duration(), track.timescale());

            let mut builder = MediaSegment::builder();
            builder
                .duration(duration)
                .uri(format!("{}/{start_time}.m4s", descriptor.id));
            if index == 0 {
                builder.map(ExtXMap::new(format!("{}/init.mp4", descriptor.id)));
            }

            start_time = start_time
                .checked_add(segment.raw_duration())
                .ok_or_else(|| HlsError::SegmentTimeOverflow(descriptor.id.clone()))?;
            Ok(builder.build()?)
        })
        .collect::<Result<Vec<_>, HlsError>>()?;

    Ok(MediaPlaylist::builder()
        .target_duration(Duration::from_secs(target_duration))
        .playlist_type(PlaylistType::Vod)
        .has_end_list(true)
        .segments(segments)
        .build()?)
}

fn media_duration(raw_duration: u64, timescale: u32) -> Duration {
    let duration =
        (u128::from(raw_duration) * 1_000 + u128::from(timescale) / 2) / u128::from(timescale);
    Duration::from_millis(u64::try_from(duration).unwrap_or(u64::MAX))
}

fn rounded_duration_seconds(raw_duration: u64, timescale: u32) -> u64 {
    let raw_duration = u128::from(raw_duration);
    let timescale = u128::from(timescale);
    u64::try_from((raw_duration + timescale / 2) / timescale).unwrap_or(u64::MAX)
}

fn ensure_unique_rendition_names(asset: &AssetDescriptor) -> Result<(), HlsError> {
    let mut names = HashSet::new();
    for descriptor in &asset.tracks {
        let (group_id, language, role) = match &descriptor.kind {
            TrackKind::Video(_) => continue,
            TrackKind::Audio(audio) => (AUDIO_GROUP_ID, &audio.language, audio.role),
            TrackKind::Text(text) => (SUBTITLES_GROUP_ID, &text.language, text.role),
        };
        let name = roles::name(language, role);
        if !names.insert((group_id, name.clone())) {
            return Err(HlsError::DuplicateRenditionName(name));
        }
    }
    Ok(())
}

fn build_master_playlist(
    asset: &AssetDescriptor,
    tracks: &[Track],
    segment_options: &SegmentOptions,
) -> Result<MasterPlaylist<'static>, HlsError> {
    let has_audio = tracks
        .iter()
        .any(|track| matches!(track.kind(), TrackKind::Audio(_)));
    let has_subtitles = tracks
        .iter()
        .any(|track| matches!(track.kind(), TrackKind::Text(_)));
    let rendition_codecs = tracks
        .iter()
        .filter(|track| !matches!(track.kind(), TrackKind::Video(_)))
        .map(Track::codec)
        .collect::<Vec<_>>();
    let rendition_bandwidth = max_rendition_bandwidth(tracks, segment_options, |kind| {
        matches!(kind, TrackKind::Audio(_))
    })
    .saturating_add(max_rendition_bandwidth(tracks, segment_options, |kind| {
        matches!(kind, TrackKind::Text(_))
    }));
    let rendition_average_bandwidth =
        max_rendition_average_bandwidth(tracks, segment_options, |kind| {
            matches!(kind, TrackKind::Audio(_))
        })
        .saturating_add(max_rendition_average_bandwidth(
            tracks,
            segment_options,
            |kind| matches!(kind, TrackKind::Text(_)),
        ));

    let variants = asset
        .tracks
        .iter()
        .zip(tracks)
        .filter_map(|(descriptor, track)| {
            let TrackKind::Video(video) = track.kind() else {
                return None;
            };
            let frame_rate = match frame_rate(&video.frame_rate) {
                Ok(frame_rate) => frame_rate,
                Err(error) => return Some(Err(error)),
            };

            let codecs = unique_codecs(
                std::iter::once(track.codec()).chain(rendition_codecs.iter().copied()),
            );
            let bandwidth = max_bitrate(track, segment_options).saturating_add(rendition_bandwidth);
            let average_bandwidth =
                average_bitrate(track, segment_options).saturating_add(rendition_average_bandwidth);
            let mut stream_data = StreamData::builder();
            stream_data
                .bandwidth(bandwidth)
                .average_bandwidth(average_bandwidth)
                .codecs(codecs)
                .resolution((video.width as usize, video.height as usize));

            Some(
                stream_data
                    .build()
                    .map_err(HlsError::from)
                    .map(|stream_data| VariantStream::ExtXStreamInf {
                        uri: Cow::Owned(format!("{}.m3u8", descriptor.id)),
                        frame_rate: Some(frame_rate),
                        audio: has_audio.then_some(Cow::Borrowed(AUDIO_GROUP_ID)),
                        subtitles: has_subtitles.then_some(Cow::Borrowed(SUBTITLES_GROUP_ID)),
                        closed_captions: Some(ClosedCaptions::None),
                        stream_data,
                    }),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(MasterPlaylist::builder()
        .media(media_entries(asset)?)
        .variant_streams(variants)
        .has_independent_segments(true)
        .build()?)
}

fn frame_rate(value: &str) -> Result<UFloat, HlsError> {
    let (numerator, denominator) = value
        .split_once('/')
        .ok_or_else(|| HlsError::InvalidFrameRate(value.to_string()))?;
    let numerator = numerator
        .parse::<u32>()
        .map_err(|_| HlsError::InvalidFrameRate(value.to_string()))?;
    let denominator = denominator
        .parse::<u32>()
        .map_err(|_| HlsError::InvalidFrameRate(value.to_string()))?;
    if numerator == 0 || denominator == 0 {
        return Err(HlsError::InvalidFrameRate(value.to_string()));
    }

    let rate = f64::from(numerator) / f64::from(denominator);
    let rounded = (rate * 1000.0).round() / 1000.0;
    UFloat::try_from(rounded as f32).map_err(|_| HlsError::InvalidFrameRate(value.to_string()))
}

fn unique_codecs<'a>(codecs: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    codecs.into_iter().fold(Vec::new(), |mut unique, codec| {
        if !unique.iter().any(|existing| existing == codec) {
            unique.push(codec.to_string());
        }
        unique
    })
}

fn media_entries(asset: &AssetDescriptor) -> Result<Vec<ExtXMedia<'static>>, hls_m3u8::Error> {
    let default_audio_id = asset
        .tracks
        .iter()
        .find(|descriptor| {
            matches!(
                descriptor.kind,
                TrackKind::Audio(ref audio) if audio.role == Some(dyndo_core::role::Role::Main)
            )
        })
        .or_else(|| {
            asset.tracks.iter().find(|descriptor| {
                matches!(descriptor.kind, TrackKind::Audio(ref audio) if audio.role.is_none())
            })
        })
        .map(|descriptor| descriptor.id.as_str());

    asset
        .tracks
        .iter()
        .filter_map(|descriptor| media_entry(asset, descriptor, default_audio_id))
        .collect::<Result<Vec<_>, _>>()
}

fn media_entry(
    asset: &AssetDescriptor,
    descriptor: &TrackDescriptor,
    default_audio_id: Option<&str>,
) -> Option<Result<ExtXMedia<'static>, hls_m3u8::Error>> {
    let (media_type, group_id, language, role, channels) = match &descriptor.kind {
        TrackKind::Video(_) => return None,
        TrackKind::Audio(audio) => (
            MediaType::Audio,
            AUDIO_GROUP_ID,
            &audio.language,
            audio.role,
            Some(Channels::new(u64::from(audio.channels))),
        ),
        TrackKind::Text(text) => (
            MediaType::Subtitles,
            SUBTITLES_GROUP_ID,
            &text.language,
            text.role,
            None,
        ),
    };
    let is_default = default_audio_id == Some(descriptor.id.as_str());
    let is_autoselect = is_default || selection_tuple_is_unique(asset, descriptor);

    let mut builder = ExtXMedia::builder();
    builder
        .media_type(media_type)
        .uri(format!("{}.m3u8", descriptor.id))
        .group_id(group_id)
        .language(language.to_string())
        .name(roles::name(language, role))
        .is_default(is_default)
        .is_autoselect(is_autoselect)
        .is_forced(roles::is_forced(role));
    if let Some(channels) = channels {
        builder.channels(channels);
    }
    if let Some(characteristics) = roles::characteristics(role) {
        builder.characteristics(characteristics);
    }

    Some(builder.build())
}

fn selection_tuple_is_unique(asset: &AssetDescriptor, descriptor: &TrackDescriptor) -> bool {
    let Some((is_audio, language, role)) = selection_tuple(descriptor) else {
        return false;
    };

    asset
        .tracks
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

fn selection_tuple(
    descriptor: &TrackDescriptor,
) -> Option<(bool, &LanguageTag, Option<dyndo_core::role::Role>)> {
    match &descriptor.kind {
        TrackKind::Video(_) => None,
        TrackKind::Audio(audio) => Some((true, &audio.language, audio.role)),
        TrackKind::Text(text) => Some((false, &text.language, text.role)),
    }
}

fn max_rendition_bandwidth(
    tracks: &[Track],
    segment_options: &SegmentOptions,
    include: impl Fn(&TrackKind) -> bool,
) -> u64 {
    tracks
        .iter()
        .filter(|track| include(track.kind()))
        .map(|track| max_bitrate(track, segment_options))
        .max()
        .unwrap_or(0)
}

fn max_rendition_average_bandwidth(
    tracks: &[Track],
    segment_options: &SegmentOptions,
    include: impl Fn(&TrackKind) -> bool,
) -> u64 {
    tracks
        .iter()
        .filter(|track| include(track.kind()))
        .map(|track| average_bitrate(track, segment_options))
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_rate_rounds_to_three_decimal_places() {
        assert_eq!(frame_rate("30000/1001").unwrap().as_f32(), 29.97);
    }

    #[test]
    fn frame_rate_rejects_a_zero_denominator() {
        assert!(matches!(
            frame_rate("25/0"),
            Err(HlsError::InvalidFrameRate(_))
        ));
    }

    #[test]
    fn unique_codecs_preserves_first_seen_order() {
        assert_eq!(
            unique_codecs(["avc1.640028", "mp4a.40.2", "mp4a.40.2"]),
            ["avc1.640028", "mp4a.40.2"]
        );
    }

    #[test]
    fn media_duration_rounds_to_milliseconds() {
        assert_eq!(
            media_duration(3_280_499, 1_000_000),
            Duration::from_millis(3_280)
        );
    }

    #[test]
    fn serialize_media_playlist_rounds_extinf_to_three_decimals() {
        let segment = MediaSegment::builder()
            .duration(media_duration(3_280, 1_000))
            .uri("segment.m4s")
            .build()
            .unwrap();
        let playlist = MediaPlaylist::builder()
            .target_duration(Duration::from_secs(3))
            .segments(vec![segment])
            .build()
            .unwrap();

        assert!(serialize_media_playlist(&playlist).contains("#EXTINF:3.280,"));
    }

    #[test]
    fn rounded_duration_seconds_rounds_half_up() {
        assert_eq!(rounded_duration_seconds(6_500, 1_000), 7);
    }

    #[test]
    fn ensure_unique_rendition_names_rejects_duplicates_within_a_group() {
        let asset: AssetDescriptor = serde_json::from_value(serde_json::json!({
            "tracks": [
                {
                    "id": "audio-1",
                    "path": "audio-1.mp4",
                    "codec": "mp4a.40.2",
                    "type": "audio",
                    "sample_rate": 48000,
                    "channels": 2,
                    "language": "en",
                    "role": "main"
                },
                {
                    "id": "audio-2",
                    "path": "audio-2.mp4",
                    "codec": "mp4a.40.2",
                    "type": "audio",
                    "sample_rate": 48000,
                    "channels": 2,
                    "language": "en",
                    "role": "main"
                }
            ]
        }))
        .unwrap();

        assert!(matches!(
            ensure_unique_rendition_names(&asset),
            Err(HlsError::DuplicateRenditionName(_))
        ));
    }
}
