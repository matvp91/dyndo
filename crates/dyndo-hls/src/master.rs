use std::borrow::Cow;

use dyndo_core::role::Role;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::served_segment::ServedSegment;
use dyndo_core::track::Track;
use dyndo_core::track_kind::{TrackKind, VideoKind};
use hls_m3u8::MasterPlaylist;
use hls_m3u8::builder::MasterPlaylistBuilder;
use hls_m3u8::tags::{ExtXMedia, VariantStream};
use hls_m3u8::types::{Channels, ClosedCaptions, MediaType, StreamData, UFloat};
use language_tags::LanguageTag;

use crate::HlsError;
use crate::options::HlsOptions;
use crate::roles;

const AUDIO_GROUP_ID: &str = "audio";
const SUBTITLES_GROUP_ID: &str = "subtitles";

struct Renditions {
    has_audio: bool,
    has_subtitles: bool,
    codecs: Vec<String>,
    maximum_bitrate: u64,
    average_bitrate: u64,
}

impl Renditions {
    fn summarize(
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

            let segments = served_segments(track, segment_options);
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

    fn codecs_for(&self, track: &Track) -> Vec<String> {
        let mut codecs = Vec::with_capacity(self.codecs.len() + 1);
        push_unique(&mut codecs, track.codec().rfc6381());
        for codec in &self.codecs {
            push_unique(&mut codecs, codec.clone());
        }
        codecs
    }
}

pub(crate) fn build_playlist(
    tracks: &[Track],
    segment_options: &SegmentOptions,
    hls_options: &HlsOptions,
) -> Result<MasterPlaylistBuilder<'static>, HlsError> {
    let renditions = Renditions::summarize(tracks, segment_options, hls_options);
    let mut builder = MasterPlaylist::builder();
    builder
        .media(build_media_entries(tracks)?)
        .variant_streams(build_variant_streams(tracks, segment_options, &renditions)?)
        .unknown_tags(image_streams(tracks, hls_options))
        .has_independent_segments(true);
    Ok(builder)
}

fn image_streams(tracks: &[Track], hls_options: &HlsOptions) -> Vec<Cow<'static, str>> {
    tracks
        .iter()
        .find(|track| matches!(track.kind(), TrackKind::Video(_)))
        .and_then(|track| crate::image::stream_inf(track, hls_options))
        .map(Cow::Owned)
        .into_iter()
        .collect()
}

fn build_variant_streams(
    tracks: &[Track],
    segment_options: &SegmentOptions,
    renditions: &Renditions,
) -> Result<Vec<VariantStream<'static>>, HlsError> {
    tracks
        .iter()
        .filter_map(|track| match track.kind() {
            TrackKind::Video(video) => Some(build_variant_stream(
                track,
                video,
                segment_options,
                renditions,
            )),
            TrackKind::Audio(_) | TrackKind::Text(_) => None,
        })
        .collect()
}

fn build_variant_stream(
    track: &Track,
    video: &VideoKind,
    segment_options: &SegmentOptions,
    renditions: &Renditions,
) -> Result<VariantStream<'static>, HlsError> {
    let segments = served_segments(track, segment_options);
    let mut stream_data = StreamData::builder();
    stream_data
        .bandwidth(
            ServedSegment::maximum_bitrate(&segments).saturating_add(renditions.maximum_bitrate),
        )
        .average_bandwidth(
            ServedSegment::average_bitrate(&segments).saturating_add(renditions.average_bitrate),
        )
        .codecs(renditions.codecs_for(track))
        .resolution((video.width as usize, video.height as usize));

    Ok(VariantStream::ExtXStreamInf {
        uri: Cow::Owned(format!("{}.m3u8", crate::media_resource_name(track))),
        frame_rate: Some(frame_rate(&video.frame_rate)?),
        audio: renditions
            .has_audio
            .then_some(Cow::Borrowed(AUDIO_GROUP_ID)),
        subtitles: renditions
            .has_subtitles
            .then_some(Cow::Borrowed(SUBTITLES_GROUP_ID)),
        closed_captions: Some(ClosedCaptions::None),
        stream_data: stream_data.build()?,
    })
}

fn build_media_entries(tracks: &[Track]) -> Result<Vec<ExtXMedia<'static>>, hls_m3u8::Error> {
    let default_audio_id = default_audio_id(tracks);
    tracks
        .iter()
        .filter_map(|track| build_media_entry(tracks, track, default_audio_id))
        .collect()
}

fn build_media_entry(
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
        .uri(format!("{}.m3u8", crate::media_resource_name(track)))
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

fn push_unique(codecs: &mut Vec<String>, codec: String) {
    if !codecs.contains(&codec) {
        codecs.push(codec);
    }
}

fn served_segments<'a>(track: &'a Track, options: &SegmentOptions) -> Vec<ServedSegment<'a>> {
    ServedSegment::group(track.segments(), options.min_length, &options.boundaries)
}
