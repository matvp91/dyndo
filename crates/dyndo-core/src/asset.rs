use std::time::Duration;

use relative_path::RelativePathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::{DurationSecondsWithFrac, serde_as};

use crate::track::Track;

pub const ASSET_SCHEMA_URL: &str = concat!(
    "https://matvp91.github.io/dyndo/",
    env!("CARGO_PKG_VERSION"),
    "/schema.json"
);

#[serde_as]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    #[serde(rename = "$schema", default = "asset_schema_url")]
    schema: String,
    #[serde(skip)]
    #[schemars(skip)]
    path: RelativePathBuf,
    /// Splice points, in seconds from the presentation start.
    #[serde_as(as = "Vec<DurationSecondsWithFrac<f64>>")]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boundaries: Vec<Duration>,
    pub tracks: Vec<Track>,
}

impl Asset {}

impl Default for Asset {
    fn default() -> Self {
        Self {
            schema: asset_schema_url(),
            path: RelativePathBuf::default(),
            boundaries: Vec::new(),
            tracks: Vec::new(),
        }
    }
}

fn asset_schema_url() -> String {
    ASSET_SCHEMA_URL.to_owned()
}
