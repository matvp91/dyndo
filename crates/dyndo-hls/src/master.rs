use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::cmaf::{ResolvedCmafTrack, ServedSegment};
use dyndo_core::track::metadata::VideoMetadata;
use dyndo_core::track::thumbnail::ResolvedThumbnailTrack;
use m3u8_rs::{ClosedCaptionGroupId, ExtTag, MasterPlaylist, Resolution, VariantStream};

use crate::HlsError;
use crate::options::HlsOptions;
use crate::renditions::Renditions;

const AUDIO_GROUP_ID: &str = "audio";
const SUBTITLES_GROUP_ID: &str = "subtitles";

pub(crate) fn build_playlist(
    tracks: &[ResolvedCmafTrack],
    thumbnails: &[ResolvedThumbnailTrack],
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

fn image_streams(thumbnails: &[ResolvedThumbnailTrack]) -> Vec<ExtTag> {
    thumbnails
        .iter()
        .map(|thumbnail| {
            let (width, height) = thumbnail.tile_dimensions();
            ExtTag {
                tag: "X-IMAGE-STREAM-INF".to_string(),
                rest: Some(format!(
                    "BANDWIDTH={},CODECS=\"jpeg\",RESOLUTION={width}x{height},URI=\"{}.m3u8\"",
                    thumbnail.bandwidth(),
                    thumbnail.id(),
                )),
            }
        })
        .collect()
}

fn build_variant_streams(
    tracks: &[ResolvedCmafTrack],
    segment_options: &SegmentOptions,
    renditions: &Renditions,
) -> Result<Vec<VariantStream>, HlsError> {
    tracks
        .iter()
        .filter_map(|track| {
            track
                .kind()
                .video()
                .map(|video| build_variant_stream(track, video, segment_options, renditions))
        })
        .collect()
}

fn build_variant_stream(
    track: &ResolvedCmafTrack,
    video: &VideoMetadata,
    segment_options: &SegmentOptions,
    renditions: &Renditions,
) -> Result<VariantStream, HlsError> {
    let segments = track.served_segments(segment_options);
    Ok(VariantStream {
        is_i_frame: false,
        uri: format!("{}.m3u8", track.id()),
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
