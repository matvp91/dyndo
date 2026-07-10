//! CMAF header parsing: scans the `moov`/`sidx`/first-`moof` boxes of a
//! single-track fragmented MP4 into a [`CmafHeader`], plus the segment-map and
//! segment-read helpers derived from it.

mod boxes;
mod codec;
mod header;
mod segment;
mod stream;

pub use codec::{AudioCodec, VideoCodec};
pub use header::{read_header, CmafHeader};
pub use segment::{find_segment_by_time, read_init_segment, read_segment, Segment};
pub use stream::{AudioStream, Stream, VideoStream};
