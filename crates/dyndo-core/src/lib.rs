mod asset;
pub mod cmaf;
mod error;
mod model;
mod storage;

pub use asset::{build_asset, describe_track};
pub use cmaf::{read_header, ByteRange, CmafHeader, Segment, TrackMeta};
pub use error::{Error, Result};
pub use model::{Asset, AudioTrack, Track, VideoTrack};
pub use storage::{LocalFile, S3Source, Source};
