use std::borrow::Cow;

use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::segment_options::SegmentOptions;
use opendal::Operator;
use serde::{Deserialize, de::DeserializeOwned};

use crate::error::ServerError;

#[derive(Debug, Deserialize)]
pub(super) struct RequestContext<T> {
    #[serde(alias = "a")]
    pub(super) asset: String,
    #[serde(flatten)]
    pub(super) segment_options: SegmentOptions,
    #[serde(flatten)]
    pub(super) manifest_options: T,
}

impl<T> RequestContext<T> {
    pub(super) async fn read_asset(&self, op: &Operator) -> Result<AssetDescriptor, ServerError> {
        let mut asset = AssetDescriptor::read(op, &format!("{}.json", self.asset)).await?;
        let options = &mut asset.segment_options;
        if self.segment_options.min_length != 0 {
            options.min_length = self.segment_options.min_length;
        }
        if self.segment_options.text_length != 0 {
            options.text_length = self.segment_options.text_length;
        }
        if !self.segment_options.boundaries.is_empty() {
            options.boundaries = self.segment_options.boundaries.clone();
        }

        Ok(asset)
    }
}

pub(super) fn parse_context<T: DeserializeOwned>(
    fragment: &str,
) -> Result<RequestContext<T>, ServerError> {
    let object = if fragment.starts_with('(') {
        Cow::Borrowed(fragment)
    } else {
        Cow::Owned(format!("({fragment})"))
    };

    rison::from_str(&object).map_err(|error| ServerError::InvalidOptions(error.to_string()))
}
