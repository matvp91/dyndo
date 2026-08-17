use relative_path::{RelativePath, RelativePathBuf};

use super::{CmafTrack, TextTrack, Track, cmaf_metadata::CmafMetadata};
use crate::{
    mp4_readable::{Mp4Readable, Mp4ReadableError},
    segment_index::SegmentIndex,
    segment_timeline::SegmentTimeline,
};

pub struct AnyCmafTrack {
    bitrate: u64,
    codec: crate::codec_config::CodecConfig,
    metadata: CmafMetadata,
}

impl AnyCmafTrack {
    pub async fn discover(source_path: &RelativePath) -> Result<Self, Mp4ReadableError> {
        let (segment_index, (codec, metadata)) = tokio::try_join!(
            SegmentIndex::from_path(source_path),
            CmafMetadata::from_path(source_path),
        )?;

        Ok(Self {
            bitrate: segment_index.avg_bitrate(),
            codec,
            metadata,
        })
    }

    pub fn into_track(self, path: RelativePathBuf) -> Track {
        match self.metadata {
            CmafMetadata::Video(metadata) => Track::Video(CmafTrack {
                path,
                codec: self.codec,
                bitrate: self.bitrate,
                metadata,
            }),
            CmafMetadata::Audio(metadata) => Track::Audio(CmafTrack {
                path,
                codec: self.codec,
                bitrate: self.bitrate,
                metadata,
            }),
            CmafMetadata::Text(metadata) => Track::Text(TextTrack::Cmaf(CmafTrack {
                path,
                codec: self.codec,
                bitrate: self.bitrate,
                metadata,
            })),
        }
    }
}
