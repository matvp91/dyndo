//! HLS playlist construction from dyndo assets.

use std::borrow::Cow;
use std::time::Duration;

use dyndo_core::asset_descriptor::{AssetDescriptor, TrackDescriptor, TrackKind};
use dyndo_core::filter::{Filter, FilterMatchedNothing};
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
    #[error(transparent)]
    Filter(#[from] FilterMatchedNothing),
}

/// Generates an HLS multivariant playlist containing the asset's video tracks.
///
/// `filter` narrows which of the asset's tracks the playlist describes; pass `None`
/// to describe all of them.
///
/// # Errors
///
/// Returns an error when a track cannot be probed, the filter matches no track, or
/// the resulting playlist is rejected by `hls_m3u8`.
pub async fn generate_master_playlist(
    op: &Operator,
    asset: &AssetDescriptor,
    hls_options: &HlsOptions,
    filter: Option<&Filter>,
) -> Result<MasterPlaylist<'static>, HlsError> {
    let tracks = Track::probe_all(op, asset).await?;
    let tracks = match filter {
        Some(filter) => filter.narrow(tracks, &asset.segment_options)?,
        None => tracks,
    };

    build_master_playlist(&tracks, &asset.segment_options, hls_options)
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
    hls_options: &HlsOptions,
) -> Result<MediaPlaylist<'static>, HlsError> {
    let path = asset.track_path(descriptor);
    let segment_options = &asset.segment_options;
    let track = Track::probe(op, &path, Some(descriptor), segment_options).await?;
    build_media_playlist(&track, segment_options, hls_options)
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

/// Whether HLS serves a track of this kind as plain WebVTT documents rather than
/// as packaged CMAF `wvtt` segments.
///
/// Only a text track has cues to serve, and a text source is a WebVTT document,
/// so the request's option decides the rest. A plain WebVTT rendition carries no
/// initialization segment and no codec, which is what the two callers act on.
fn serves_plain_vtt(kind: &TrackKind, options: &HlsOptions) -> bool {
    !options.wvtt && matches!(kind, TrackKind::Text(_))
}

fn build_media_playlist(
    track: &Track,
    segment_options: &SegmentOptions,
    hls_options: &HlsOptions,
) -> Result<MediaPlaylist<'static>, HlsError> {
    let plain_vtt = serves_plain_vtt(track.kind(), hls_options);
    let extension = if plain_vtt { "vtt" } else { "m4s" };
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
            let start_time = segment.raw_time_range().start;

            let mut builder = MediaSegment::builder();
            builder
                .duration(duration)
                .uri(format!("{}/{start_time}.{extension}", track.id()));
            if index == 0 && !plain_vtt {
                builder.map(ExtXMap::new(format!("{}/init.mp4", track.id())));
            }

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

/// Builds the playlist from tracks already probed and already narrowed.
fn build_master_playlist(
    tracks: &[Track],
    segment_options: &SegmentOptions,
    hls_options: &HlsOptions,
) -> Result<MasterPlaylist<'static>, HlsError> {
    let has_audio = tracks
        .iter()
        .any(|track| matches!(track.kind(), TrackKind::Audio(_)));
    let has_subtitles = tracks
        .iter()
        .any(|track| matches!(track.kind(), TrackKind::Text(_)));
    // A plain WebVTT rendition has no codec to advertise, and naming `wvtt` would
    // tell a player to expect a packaged track instead.
    let rendition_codecs = tracks
        .iter()
        .filter(|track| !matches!(track.kind(), TrackKind::Video(_)))
        .filter(|track| !serves_plain_vtt(track.kind(), hls_options))
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

    let variants = tracks
        .iter()
        .filter_map(|track| {
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
                        uri: Cow::Owned(format!("{}.m3u8", track.id())),
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
        .media(media_entries(tracks)?)
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

fn media_entries(tracks: &[Track]) -> Result<Vec<ExtXMedia<'static>>, hls_m3u8::Error> {
    let default_audio_id = tracks
        .iter()
        .find(|track| {
            matches!(
                track.kind(),
                TrackKind::Audio(audio) if audio.role == Some(dyndo_core::role::Role::Main)
            )
        })
        .or_else(|| {
            tracks.iter().find(
                |track| matches!(track.kind(), TrackKind::Audio(audio) if audio.role.is_none()),
            )
        })
        .map(Track::id);

    tracks
        .iter()
        .filter_map(|track| media_entry(tracks, track, default_audio_id))
        .collect::<Result<Vec<_>, _>>()
}

fn media_entry(
    tracks: &[Track],
    track: &Track,
    default_audio_id: Option<&str>,
) -> Option<Result<ExtXMedia<'static>, hls_m3u8::Error>> {
    let (media_type, group_id, language, role, channels) = match track.kind() {
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
    let is_default = default_audio_id == Some(track.id());
    let is_autoselect = is_default || selection_tuple_is_unique(tracks, track);

    let mut builder = ExtXMedia::builder();
    builder
        .media_type(media_type)
        .uri(format!("{}.m3u8", track.id()))
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

fn selection_tuple(track: &Track) -> Option<(bool, &LanguageTag, Option<dyndo_core::role::Role>)> {
    match track.kind() {
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
    use opendal::services::Memory;

    use super::*;

    const DOCUMENT: &str = "WEBVTT\n\n00:00.000 --> 00:02.000\nHello\n";

    #[tokio::test]
    async fn a_text_track_carries_vtt_segments_and_no_initialization_map() {
        let (op, asset) = subtitle_asset().await;

        let playlist =
            generate_media_playlist(&op, &asset, &asset.tracks[0], &HlsOptions::default())
                .await
                .unwrap();
        let serialized = serialize_media_playlist(&playlist);

        assert!(
            serialized.contains("text-nld/0.vtt") && !serialized.contains("EXT-X-MAP"),
            "unexpected playlist:\n{serialized}"
        );
    }

    #[tokio::test]
    async fn a_text_track_keeps_packaged_segments_when_wvtt_is_asked_for() {
        let (op, asset) = subtitle_asset().await;
        let options = HlsOptions { wvtt: true };

        let playlist = generate_media_playlist(&op, &asset, &asset.tracks[0], &options)
            .await
            .unwrap();
        let serialized = serialize_media_playlist(&playlist);

        assert!(
            serialized.contains("text-nld/0.m4s") && serialized.contains("text-nld/init.mp4"),
            "unexpected playlist:\n{serialized}"
        );
    }

    #[test]
    fn plain_vtt_serves_a_text_track_unless_wvtt_is_asked_for() {
        let text = text_kind();

        assert!(serves_plain_vtt(&text, &HlsOptions::default()));
        assert!(!serves_plain_vtt(&text, &HlsOptions { wvtt: true }));
    }

    #[test]
    fn plain_vtt_never_serves_a_track_that_is_not_text() {
        let audio = TrackKind::Audio(
            serde_json::from_value(serde_json::json!({"sample_rate": 48000, "channels": 2}))
                .unwrap(),
        );

        assert!(!serves_plain_vtt(&audio, &HlsOptions::default()));
    }

    fn text_kind() -> TrackKind {
        TrackKind::Text(serde_json::from_value(serde_json::json!({"language": "nld"})).unwrap())
    }

    /// An asset of one raw `.vtt` track, held in memory.
    async fn subtitle_asset() -> (Operator, AssetDescriptor) {
        let op = Operator::new(Memory::default()).unwrap();
        op.write("subtitles.vtt", DOCUMENT).await.unwrap();
        let asset: AssetDescriptor = serde_json::from_value(serde_json::json!({
            "tracks": [
                {
                    "id": "text-nld",
                    "path": "subtitles.vtt",
                    "codec": "wvtt",
                    "type": "text",
                    "language": "nld"
                }
            ]
        }))
        .unwrap();

        (op, asset)
    }

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
}
