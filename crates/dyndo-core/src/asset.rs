use std::collections::HashSet;
use std::path::PathBuf;

use crate::cmaf::{read_header, Stream};
use crate::error::{Error, Result};
use crate::model::id::track_id;
use crate::model::{Asset, AudioTrack, Track, VideoTrack};
use crate::storage::{LocalFile, Source};

pub async fn describe_track<S: Source>(source: &S, key: String) -> Result<Track> {
    let header = read_header(source, &key).await?;
    let id = track_id(&header);

    // `bandwidth` fed the id (via its kbps) but is not serialized — a manifest
    // generator re-derives it from the source.
    let fourcc = header.stream.fourcc().to_string();
    let timescale = header.timescale;
    let track = match header.stream {
        Stream::Video(v) => Track::Video(VideoTrack {
            id,
            source: key,
            fourcc,
            timescale,
            width: v.width,
            height: v.height,
        }),
        Stream::Audio(a) => Track::Audio(AudioTrack {
            id,
            source: key,
            fourcc,
            timescale,
            sample_rate: a.sample_rate,
            channels: a.channels,
            language: a.language,
        }),
    };
    Ok(track)
}

pub async fn build_asset(inputs: &[PathBuf]) -> Result<Asset> {
    let mut tracks = Vec::with_capacity(inputs.len());
    let mut seen = HashSet::new();
    for path in inputs {
        let key = path.to_string_lossy().into_owned();
        let source = LocalFile::new(path);
        let track = describe_track(&source, key).await?;
        if !seen.insert(track.id().to_string()) {
            return Err(Error::DuplicateTrackId(track.id().to_string()));
        }
        tracks.push(track);
    }

    Ok(Asset { tracks })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> LocalFile {
        LocalFile::new(format!(
            "{}/tests/fixtures/{}",
            env!("CARGO_MANIFEST_DIR"),
            name
        ))
    }

    #[tokio::test]
    async fn describes_video_track_with_computed_bandwidth_and_id() {
        let src = fixture("video_avc_1080.mp4");
        let track = describe_track(&src, "video_avc_1080.mp4".into())
            .await
            .unwrap();
        match track {
            Track::Video(v) => {
                // id still embeds kbps(bandwidth) though bandwidth isn't serialized.
                assert_eq!(v.id, "video_avc_1080_4807");
                assert_eq!(v.fourcc, "avc1");
                assert_eq!(v.source, "video_avc_1080.mp4");
            }
            _ => panic!("expected video"),
        }
    }

    #[tokio::test]
    async fn describes_audio_track_with_computed_bandwidth_and_id() {
        let src = fixture("audio_aac_nl_2.mp4");
        let track = describe_track(&src, "audio_aac_nl_2.mp4".into())
            .await
            .unwrap();
        match track {
            Track::Audio(a) => {
                assert_eq!(a.id, "audio_aac_nld_2_197");
                assert_eq!(a.fourcc, "mp4a");
                assert_eq!(a.source, "audio_aac_nl_2.mp4");
                assert_eq!(a.language.as_deref(), Some("nld"));
            }
            _ => panic!("expected audio"),
        }
    }

    #[tokio::test]
    async fn build_asset_rejects_duplicate_ids() {
        // Two paths to the same fixture -> identical ids -> error.
        let base = format!(
            "{}/tests/fixtures/audio_aac_nl_2.mp4",
            env!("CARGO_MANIFEST_DIR")
        );
        let err = build_asset(&[base.clone().into(), base.into()])
            .await
            .unwrap_err();
        assert!(matches!(err, Error::DuplicateTrackId(_)));
    }
}
