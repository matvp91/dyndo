//! dyndo's domain core: parse CMAF track headers, model assets, generate
//! manifests, and read segment bytes through an OpenDAL operator.
#![deny(missing_docs)]

pub mod asset;
mod box_reader;
pub mod codec;
pub mod dash;
pub mod error;
pub mod format;
pub mod header;
pub mod header_cmaf;
pub mod header_raw;
pub mod hls;
pub mod metadata;
pub mod role;
pub mod segment;
mod segment_utils;
pub mod track;
mod track_wire;
