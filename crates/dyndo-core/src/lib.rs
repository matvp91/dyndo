//! dyndo's domain core: parse CMAF track headers, model assets, and read
//! segment bytes through an OpenDAL operator.
#![deny(missing_docs)]

pub mod asset;
pub mod cmaf;
pub mod codec;
pub mod dash;
mod error;
pub mod hls;
pub mod model;
pub mod text;

pub use error::CoreError;
