use std::borrow::Cow;

use dyndo_core::asset_descriptor::AssetDescriptor;
use serde::Deserialize;

use crate::error::ServerError;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Options {
    #[serde(alias = "a")]
    asset: String,
    #[serde(default, alias = "sml", alias = "segment_min_length")]
    min_length: u32,
    #[serde(default, alias = "stl", alias = "segment_text_length")]
    text_length: u32,
    #[serde(default, alias = "sb", alias = "segment_boundaries")]
    boundaries: Vec<u32>,
    #[serde(default, alias = "c")]
    compact: bool,
    #[serde(default, alias = "mp")]
    multi_period: bool,
    #[serde(default, alias = "tts")]
    thumbnail_tile_size: u32,
    #[serde(default, alias = "ts")]
    thumbnail_step: u32,
    #[serde(default)]
    wvtt: bool,
}

impl Options {
    pub(super) fn parse(fragment: &str) -> Result<Self, ServerError> {
        let object = if fragment.starts_with('(') {
            Cow::Borrowed(fragment)
        } else {
            Cow::Owned(format!("({fragment})"))
        };

        rison::from_str(&object)
            .map_err(|error| ServerError::BadRequest(format!("invalid options: {error}")))
    }

    pub(super) fn apply_to(&self, descriptor: &mut AssetDescriptor) {
        let options = &mut descriptor.segment_options;
        if self.min_length != 0 {
            options.min_length = self.min_length;
        }
        if self.text_length != 0 {
            options.text_length = self.text_length;
        }
        if !self.boundaries.is_empty() {
            options.boundaries.clone_from(&self.boundaries);
        }
    }

    pub(super) fn asset(&self) -> &str {
        &self.asset
    }

    pub(super) fn thumbnail_settings(&self) -> (u32, u32) {
        (self.thumbnail_tile_size, self.thumbnail_step)
    }

    pub(super) fn dash_options(&self) -> dyndo_dash::options::DashOptions {
        dyndo_dash::options::DashOptions {
            compact: self.compact,
            multi_period: self.multi_period,
            thumbnail_tile_size: self.thumbnail_tile_size,
            thumbnail_step: self.thumbnail_step,
        }
    }

    pub(super) fn hls_options(&self) -> dyndo_hls::options::HlsOptions {
        dyndo_hls::options::HlsOptions { wvtt: self.wvtt }
    }
}

#[cfg(test)]
mod tests {
    use dyndo_core::asset_descriptor::AssetDescriptor;
    use dyndo_core::segment_options::SegmentOptions;

    use super::Options;

    #[test]
    fn parse_reads_root_segment_options() {
        let options =
            Options::parse("asset:demo,min_length:1000,text_length:2000,boundaries:!(3000)")
                .unwrap();

        assert_eq!(
            (options.min_length, options.text_length, options.boundaries),
            (1_000, 2_000, vec![3_000])
        );
    }

    #[test]
    fn apply_to_preserves_descriptor_values_when_options_are_empty() {
        let options = Options::parse("asset:demo").unwrap();
        let mut descriptor = AssetDescriptor::default();
        descriptor.segment_options = SegmentOptions {
            min_length: 1_000,
            text_length: 2_000,
            boundaries: vec![3_000],
        };

        options.apply_to(&mut descriptor);

        assert_eq!(
            descriptor.segment_options,
            SegmentOptions {
                min_length: 1_000,
                text_length: 2_000,
                boundaries: vec![3_000],
            }
        );
    }

    #[test]
    fn apply_to_overwrites_descriptor_values() {
        let options =
            Options::parse("asset:demo,min_length:1000,text_length:2000,boundaries:!(3000)")
                .unwrap();
        let mut descriptor = AssetDescriptor::default();

        options.apply_to(&mut descriptor);

        assert_eq!(
            descriptor.segment_options,
            SegmentOptions {
                min_length: 1_000,
                text_length: 2_000,
                boundaries: vec![3_000],
            }
        );
    }

    #[test]
    fn dash_options_returns_builder_configuration() {
        let options = Options::parse("asset:demo,c:!t,mp:!t,tts:2,ts:1000").unwrap();

        assert_eq!(
            options.dash_options(),
            dyndo_dash::options::DashOptions {
                compact: true,
                multi_period: true,
                thumbnail_tile_size: 2,
                thumbnail_step: 1_000,
            }
        );
    }

    #[test]
    fn hls_options_returns_builder_configuration() {
        let options = Options::parse("asset:demo,wvtt:!t").unwrap();

        assert_eq!(
            options.hls_options(),
            dyndo_hls::options::HlsOptions { wvtt: true }
        );
    }
}
