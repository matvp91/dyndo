mod codec;
mod header;

pub use header::{read_header, ByteRange, CmafHeader, Segment, TrackMeta};
