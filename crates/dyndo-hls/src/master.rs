use dyndo_core::image::Thumbnail;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::served_segment::ServedSegment;
use dyndo_core::track::Track;
use dyndo_core::track_kind::{TrackKind, VideoKind};
use m3u8_rs::{ClosedCaptionGroupId, ExtTag, MasterPlaylist, Resolution, VariantStream};

use crate::HlsError;
use crate::options::HlsOptions;
use crate::renditions::Renditions;

const AUDIO_GROUP_ID: &str = "audio";
const SUBTITLES_GROUP_ID: &str = "subtitles";

pub(crate) fn build_playlist(
    tracks: &[Track],
    thumbnails: &[Thumbnail<'_>],
    segment_options: &SegmentOptions,
    hls_options: &HlsOptions,
) -> Result<MasterPlaylist, HlsError> {
    let renditions = Renditions::summarize(tracks, segment_options, hls_options);
    Ok(MasterPlaylist {
        version: None,
        variants: build_variant_streams(tracks, segment_options, &renditions)?,
        session_data: Vec::new(),
        session_key: Vec::new(),
        start: None,
        independent_segments: true,
        alternatives: Renditions::media_entries(tracks),
        unknown_tags: image_streams(thumbnails),
    })
}

fn image_streams(thumbnails: &[Thumbnail<'_>]) -> Vec<ExtTag> {
    thumbnails.iter().map(crate::image::stream_inf).collect()
}

fn build_variant_streams(
    tracks: &[Track],
    segment_options: &SegmentOptions,
    renditions: &Renditions,
) -> Result<Vec<VariantStream>, HlsError> {
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
) -> Result<VariantStream, HlsError> {
    let segments = served_segments(track, segment_options);
    Ok(VariantStream {
        is_i_frame: false,
        uri: format!("{}.m3u8", crate::media_resource_name(track)),
        bandwidth: ServedSegment::maximum_bitrate(&segments)
            .saturating_add(renditions.maximum_bitrate),
        average_bandwidth: Some(
            ServedSegment::average_bitrate(&segments).saturating_add(renditions.average_bitrate),
        ),
        codecs: Some(renditions.codecs_for(track).join(",")),
        resolution: Some(Resolution {
            width: u64::from(video.width),
            height: u64::from(video.height),
        }),
        frame_rate: Some(frame_rate(&video.frame_rate)?),
        hdcp_level: None,
        audio: renditions.has_audio.then(|| AUDIO_GROUP_ID.to_string()),
        video: None,
        subtitles: renditions
            .has_subtitles
            .then(|| SUBTITLES_GROUP_ID.to_string()),
        closed_captions: Some(ClosedCaptionGroupId::None),
        other_attributes: None,
    })
}

fn frame_rate(value: &str) -> Result<f64, HlsError> {
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

    Ok((f64::from(numerator) / f64::from(denominator) * 1000.0).round() / 1000.0)
}

fn served_segments<'a>(track: &'a Track, options: &SegmentOptions) -> Vec<ServedSegment<'a>> {
    ServedSegment::group(track.segments(), options.min_length, &options.boundaries)
}
