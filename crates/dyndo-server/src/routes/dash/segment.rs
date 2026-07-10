use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
};
use dyndo_core::{
    find_segment_by_time, read_header, Asset, CmafHeader, LocalFile, Source, Stream, Track,
};

use super::{load_asset, resolve_within};
use crate::config::Config;
use crate::error::ServerError;

pub(crate) async fn segment(
    State(config): State<Arc<Config>>,
    Path((asset_id, repr, seg)): Path<(String, String, String)>,
) -> Result<Response, ServerError> {
    let asset_dir = resolve_within(&config.assets_base_path, &asset_id)?;
    let asset = load_asset(&asset_dir, &asset_id).await?;
    let track = find_track(&asset, &repr)?;

    let abs = resolve_within(&asset_dir, track.source())?;
    let abs_str = abs.to_string_lossy().into_owned();
    let source = LocalFile::new(&abs);
    let header = read_header(&source, &abs_str).await?;

    let (start, len) = segment_byte_range(&header, &seg)?;
    let bytes = source.read_at(start, len).await?;
    Ok((
        [(header::CONTENT_TYPE, content_type(&header.stream))],
        bytes,
    )
        .into_response())
}

/// Find the track whose id matches the requested representation, or 404.
fn find_track<'a>(asset: &'a Asset, repr: &str) -> Result<&'a Track, ServerError> {
    asset
        .tracks
        .iter()
        .find(|t| t.id() == repr)
        .ok_or_else(|| ServerError::NotFound(format!("no representation {repr}")))
}

/// The `mimeType` for a track's media segments.
fn content_type(stream: &Stream) -> &'static str {
    match stream {
        Stream::Video(_) => "video/mp4",
        Stream::Audio(_) => "audio/mp4",
    }
}

/// Resolve a requested segment name to its `(start, len)` byte range in the source:
/// `init.mp4` -> the init range; `{time}.m4s` -> the segment starting at that
/// presentation time; anything else -> 404.
fn segment_byte_range(header: &CmafHeader, seg: &str) -> Result<(u64, usize), ServerError> {
    if seg == "init.mp4" {
        let r = &header.init_segment;
        Ok((r.offset, r.size as usize))
    } else if let Some(time_str) = seg.strip_suffix(".m4s") {
        let time: u64 = time_str
            .parse()
            .map_err(|_| ServerError::BadRequest(format!("invalid segment time: {seg}")))?;
        let r = find_segment_by_time(header, time)
            .ok_or_else(|| ServerError::NotFound(format!("no segment at time {time}")))?;
        Ok((r.offset, r.size as usize))
    } else {
        Err(ServerError::NotFound(format!("unknown segment: {seg}")))
    }
}

#[cfg(test)]
mod tests {
    use dyndo_core::{Segment, VideoCodec, VideoStream};

    use super::*;

    fn header() -> CmafHeader {
        CmafHeader {
            timescale: 90000,
            duration: 180000,
            bandwidth: 1000,
            earliest_presentation_time: 0,
            init_segment: Segment {
                offset: 0,
                size: 100,
                duration: 0,
            },
            segments: vec![
                Segment {
                    offset: 100,
                    size: 500,
                    duration: 90000,
                },
                Segment {
                    offset: 600,
                    size: 700,
                    duration: 90000,
                },
            ],
            stream: Stream::Video(VideoStream {
                codec: VideoCodec::Avc {
                    profile: 0x64,
                    constraints: 0,
                    level: 0x28,
                },
                width: 1920,
                height: 1080,
                frame_rate: (25, 1),
            }),
        }
    }

    #[test]
    fn init_resolves_to_the_init_segment() {
        assert_eq!(segment_byte_range(&header(), "init.mp4").unwrap(), (0, 100));
    }

    #[test]
    fn m4s_resolves_to_the_segment_at_that_time() {
        assert_eq!(segment_byte_range(&header(), "0.m4s").unwrap(), (100, 500));
        assert_eq!(
            segment_byte_range(&header(), "90000.m4s").unwrap(),
            (600, 700)
        );
    }

    #[test]
    fn unknown_names_and_bad_times_are_rejected() {
        // No `.m4s` suffix and not `init.mp4`.
        assert!(matches!(
            segment_byte_range(&header(), "cover.jpg"),
            Err(ServerError::NotFound(_))
        ));
        // Well-formed time, but no segment boundary there.
        assert!(matches!(
            segment_byte_range(&header(), "12345.m4s"),
            Err(ServerError::NotFound(_))
        ));
        // Non-numeric time.
        assert!(matches!(
            segment_byte_range(&header(), "abc.m4s"),
            Err(ServerError::BadRequest(_))
        ));
    }
}
