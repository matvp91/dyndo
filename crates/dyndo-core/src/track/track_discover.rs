use relative_path::RelativePath;
use thiserror::Error;

use super::{
    SidecarTextTrack, TextMetadata, TextTrack, Track, discovered_cmaf_track::DiscoveredCmafTrack,
};
use crate::{mp4_readable::Mp4Readable, storage::Storage};

#[derive(Debug, Error)]
pub enum DiscoverError {
    #[error("unsupported track format")]
    UnsupportedFormat,
    #[error("failed to access source: {0}")]
    Storage(#[from] crate::storage::StorageError),
    #[error("failed to read source: {0}")]
    Source(#[from] opendal::Error),
    #[error("failed to read MP4: {0}")]
    Mp4(#[from] mp4_atom::Error),
    #[error("invalid CMAF track: {0}")]
    InvalidCmaf(String),
}

pub struct TrackDiscover;

impl TrackDiscover {
    pub async fn probe(source_path: &RelativePath) -> Result<Track, DiscoverError> {
        match source_path.extension() {
            Some("mp4") => {
                let mut reader = Storage::source_op()?
                    .reader(source_path.as_str())
                    .await?
                    .into_futures_async_read(..)
                    .await?;
                let track = DiscoveredCmafTrack::from_reader(&mut reader).await?;

                Ok(track.into_track(source_path.as_str().into()))
            }
            Some("vtt") | Some("imsc") => Ok(Track::Text(TextTrack::Sidecar(
                SidecarTextTrack {
                    path: source_path.as_str().into(),
                    metadata: TextMetadata {
                        language: super::language_und(),
                        role: None,
                    },
                },
            ))),
            _ => Err(DiscoverError::UnsupportedFormat),
        }
    }
}
