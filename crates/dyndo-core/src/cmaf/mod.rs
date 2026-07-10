//! CMAF header parsing: scans the `moov`/`sidx`/first-`moof` boxes of a
//! single-track fragmented MP4 into a [`CmafHeader`], plus the segment-map
//! helpers derived from it.

mod boxes;
mod codec;
mod header;
mod segment;
mod stream;

pub use codec::{AudioCodec, VideoCodec};
pub use header::{read_header, CmafHeader};
pub use segment::{find_segment_by_time, Segment};
pub use stream::{AudioStream, Stream, VideoStream};
