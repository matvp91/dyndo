//! How a request spells a track filter.
//!
//! The expression language and the narrowing itself live in [`dyndo_core::filter`];
//! this is only the query parameter a filter arrives in.

use dyndo_core::filter::Filter;
use serde::Deserialize;

use crate::error::ServerError;

/// The only query parameter a manifest request takes.
///
/// Anything else is refused, which is what catches an unencoded `&&`: it splits the
/// query string, so `?filter=type!=video&&height%3C=720` would otherwise arrive as
/// the perfectly valid `filter=type!=video` and be served while quietly ignoring the
/// rest of what was asked for. The junk halves reach serde as unknown fields
/// instead, and axum answers `400`.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn query(filter: &str) -> ManifestQuery {
        ManifestQuery {
            filter: Some(filter.to_string()),
        }
    }

    #[test]
    fn resolve_parses_the_expression() {
        assert!(query("type==video").resolve().unwrap().is_some());
    }

    #[test]
    fn resolve_without_a_filter_yields_none() {
        assert!(ManifestQuery::default().resolve().unwrap().is_none());
    }

    #[test]
    fn resolve_reports_a_malformed_expression() {
        let error = query("heigth<=720").resolve().unwrap_err();

        assert!(
            error.to_string().contains("invalid filter"),
            "unexpected error: {error}"
        );
    }
}
