use relative_path::{RelativePath, RelativePathBuf};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::track::Track;

pub const ASSET_SCHEMA_URL: &str = concat!(
    "https://matvp91.github.io/dyndo/",
    env!("CARGO_PKG_VERSION"),
    "/schema.json"
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    #[serde(rename = "$schema", default = "asset_schema_url")]
    schema: String,
    #[serde(skip)]
    #[schemars(skip)]
    path: RelativePathBuf,
    /// Splice points, in milliseconds from the presentation start.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boundaries: Vec<u32>,
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
