//! Orchestration: parse a source's header, compute derived fields, build the
//! serde `Track`/`Asset`.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::cmaf::{read_header, TrackMeta};
use crate::error::{Error, Result};
use crate::model::id::{audio_track_id, video_track_id};
use crate::model::{Asset, AudioTrack, Track, VideoTrack};
use crate::storage::{LocalFile, Source};

/// Average bitrate in bits/s from the segment sizes and duration.
fn average_bandwidth(total_bytes: u64, duration: u64, timescale: u32) -> u32 {
    if duration == 0 {
        return 0;
    }
    let seconds = duration as f64 / timescale as f64;
    (total_bytes as f64 * 8.0 / seconds).round() as u32
}

pub async fn describe_track<S: Source>(source: &S, key: String) -> Result<Track> {
    let header = read_header(source, &key).await?;
    let total_bytes: u64 = header.segments.iter().map(|s| s.size).sum();
    let bandwidth = average_bandwidth(total_bytes, header.duration, header.timescale);

    let track = match header.track {
        TrackMeta::Video {
            codec,
            width,
            height,
            frame_rate,
        } => Track::Video(VideoTrack {
            id: video_track_id(&codec, height, bandwidth),
            source: key,
            codec,
            timescale: header.timescale,
            duration: header.duration,
            bandwidth,
            width,
            height,
            frame_rate,
        }),
        TrackMeta::Audio {
            codec,
            sample_rate,
            channels,
            language,
        } => Track::Audio(AudioTrack {
            id: audio_track_id(&codec, language.as_deref(), channels, bandwidth),
            source: key,
            codec,
            timescale: header.timescale,
            duration: header.duration,
            bandwidth,
            sample_rate,
            channels,
            language,
        }),
    };
    Ok(track)
}

pub async fn build_asset(inputs: &[PathBuf]) -> Result<Asset> {
    let mut tracks = Vec::with_capacity(inputs.len());
    for path in inputs {
        let key = path.to_string_lossy().into_owned();
        let source = LocalFile::new(path);
        tracks.push(describe_track(&source, key).await?);
    }

    let mut seen = HashSet::new();
    for track in &tracks {
        if !seen.insert(track.id().to_string()) {
            return Err(Error::DuplicateTrackId(track.id().to_string()));
        }
    }

    Ok(Asset { tracks })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::BytesSource;

    fn fixture(name: &str) -> BytesSource {
        let bytes = std::fs::read(format!(
            "{}/tests/fixtures/{}",
            env!("CARGO_MANIFEST_DIR"),
            name
        ))
        .unwrap();
        BytesSource::new(bytes)
    }

    #[tokio::test]
    async fn describes_video_track_with_computed_bandwidth_and_id() {
        let src = fixture("index_video_avc_1080.mp4");
        let track = describe_track(&src, "index_video_avc_1080.mp4".into())
            .await
            .unwrap();
        match track {
            Track::Video(v) => {
                assert_eq!(v.id, "video_avc_1080_4807");
                assert_eq!(v.bandwidth, 4807228);
                assert_eq!(v.codec, "avc1.640028");
                assert_eq!(v.source, "index_video_avc_1080.mp4");
            }
            _ => panic!("expected video"),
        }
    }

    #[tokio::test]
    async fn build_asset_rejects_duplicate_ids() {
        // Two paths to the same fixture -> identical ids -> error.
        let base = format!("{}/tests/fixtures/index_audio_aac_nl_2.mp4", env!("CARGO_MANIFEST_DIR"));
        let err = build_asset(&[base.clone().into(), base.into()]).await.unwrap_err();
        assert!(matches!(err, Error::DuplicateTrackId(_)));
    }
}
