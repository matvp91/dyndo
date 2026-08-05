//! What a request asks of dyndo: which asset, how to segment it, and whatever the
//! transport itself takes, all parsed from the rison fragment in the path.

use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::segment::SegmentOptions;
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
    pub(super) transport_options: T,
}

impl<T> RequestContext<T> {
    /// Reads the descriptor of the asset this request names, assigning the segment
    /// options the request named over the ones the asset asks for. Nothing is
    /// written back — the descriptor on disk keeps asking for what it asked for.
    ///
    /// An option left at zero — or an empty set of boundaries — names nothing,
    /// since a request cannot express the difference between an absent value and a
    /// zero one.
    pub(super) async fn read_asset(&self, op: &Operator) -> Result<AssetDescriptor, ServerError> {
        let mut asset = AssetDescriptor::read(op, &format!("{}.json", self.asset)).await?;
        let options = &mut asset.segment_options;
        if self.segment_options.min_length_ms != 0 {
            options.min_length_ms = self.segment_options.min_length_ms;
        }
        if self.segment_options.text_length_ms != 0 {
            options.text_length_ms = self.segment_options.text_length_ms;
        }
        if !self.segment_options.boundaries.is_empty() {
            options.boundaries = self.segment_options.boundaries.clone();
        }

        Ok(asset)
    }
}

/// Parses the rison fragment a request carries in its path.
pub(super) fn parse_context<T: DeserializeOwned>(
    fragment: &str,
) -> Result<RequestContext<T>, ServerError> {
    rison::from_str(fragment).map_err(|error| ServerError::InvalidOptions(error.to_string()))
}

#[cfg(test)]
mod tests {
    use dyndo_dash::options::DashOptions;
    use dyndo_hls::options::HlsOptions;

    use super::*;

    #[test]
    fn parse_context_accepts_a_nested_asset_path() {
        let context = parse_context::<DashOptions>("(asset:foo/asset)").unwrap();

        assert_eq!(context.asset, "foo/asset");
    }

    #[test]
    fn parse_context_accepts_asset_alias() {
        let context = parse_context::<DashOptions>("(a:foo/asset)").unwrap();

        assert_eq!(context.asset, "foo/asset");
    }

    #[test]
    fn parse_context_accepts_min_length() {
        let context = parse_context::<HlsOptions>("(asset:asset,min_length:3000)").unwrap();

        assert_eq!(context.segment_options.min_length_ms, 3000);
    }

    #[test]
    fn parse_context_accepts_sml_alias() {
        let context = parse_context::<HlsOptions>("(asset:asset,sml:3000)").unwrap();

        assert_eq!(context.segment_options.min_length_ms, 3000);
    }

    #[test]
    fn parse_context_accepts_text_length() {
        let context = parse_context::<HlsOptions>("(asset:asset,text_length:2000)").unwrap();

        assert_eq!(context.segment_options.text_length_ms, 2000);
    }

    #[test]
    fn parse_context_accepts_stl_alias() {
        let context = parse_context::<HlsOptions>("(asset:asset,stl:2000)").unwrap();

        assert_eq!(context.segment_options.text_length_ms, 2000);
    }

    #[test]
    fn parse_context_accepts_the_long_aliases() {
        let context = parse_context::<HlsOptions>("(asset:asset,segment_min_length:3000)").unwrap();

        assert_eq!(context.segment_options.min_length_ms, 3000);
    }

    #[test]
    fn parse_context_accepts_boundaries() {
        let context = parse_context::<HlsOptions>("(asset:asset,sb:!(1000,2000))").unwrap();

        assert_eq!(context.segment_options.boundaries, [1000, 2000]);
    }

    #[test]
    fn parse_context_accepts_compact_alias() {
        let context = parse_context::<DashOptions>("(asset:asset,c:!t)").unwrap();

        assert!(context.transport_options.compact);
    }

    #[test]
    fn parse_context_leaves_unnamed_segment_options_at_their_defaults() {
        let context = parse_context::<HlsOptions>("(asset:asset)").unwrap();

        assert_eq!(context.segment_options, SegmentOptions::default());
    }

    #[test]
    fn parse_context_rejects_a_negative_segment_length() {
        assert!(parse_context::<HlsOptions>("(asset:asset,sml:-1)").is_err());
    }
}
