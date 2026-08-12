use std::borrow::Cow;

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
    #[serde(default, alias = "c")]
    compact: bool,
    #[serde(default, alias = "mp")]
    multi_period: bool,
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

    pub(super) fn asset(&self) -> &str {
        &self.asset
    }

    pub(super) fn min_length(&self) -> u32 {
        self.min_length
    }

    pub(super) fn text_length(&self) -> u32 {
        self.text_length
    }

    pub(super) fn dash_options(&self) -> dyndo_dash::options::DashOptions {
        dyndo_dash::options::DashOptions {
            compact: self.compact,
            multi_period: self.multi_period,
        }
    }

    pub(super) fn hls_options(&self) -> dyndo_hls::options::HlsOptions {
        dyndo_hls::options::HlsOptions { wvtt: self.wvtt }
    }
}

#[cfg(test)]
mod tests {
    use super::Options;

    #[test]
    fn parse_reads_delivery_options() {
        let options = Options::parse("asset:demo,min_length:1000,text_length:2000").unwrap();

        assert_eq!((options.min_length, options.text_length), (1_000, 2_000));
    }

    #[test]
    fn parse_rejects_boundaries() {
        assert!(Options::parse("asset:demo,boundaries:!(3000)").is_err());
        assert!(Options::parse("asset:demo,sb:!(3000)").is_err());
        assert!(Options::parse("asset:demo,segment_boundaries:!(3000)").is_err());
    }

    #[test]
    fn dash_options_returns_builder_configuration() {
        let options = Options::parse("asset:demo,c:!t,mp:!t").unwrap();

        assert_eq!(
            options.dash_options(),
            dyndo_dash::options::DashOptions {
                compact: true,
                multi_period: true,
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
