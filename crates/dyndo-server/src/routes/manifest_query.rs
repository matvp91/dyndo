use serde::Deserialize;

use super::filter::Filter;
use crate::error::ServerError;

// Rejecting unknown fields prevents an unencoded `&&` from silently truncating a filter.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestQuery {
    filter: Option<String>,
}

impl ManifestQuery {
    pub(super) fn resolve(&self) -> Result<Option<Filter>, ServerError> {
        self.filter
            .as_deref()
            .map(Filter::parse)
            .transpose()
            .map_err(|error| ServerError::InvalidFilter(error.to_string()))
    }
}
