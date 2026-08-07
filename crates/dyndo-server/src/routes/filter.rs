//! How a request spells a track filter, and what an empty result means.
//!
//! The expression language itself lives in [`dyndo_core::filter`]; this is only the
//! query parameter it arrives in and the server's answer when it matches nothing.

use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::filter::Filter;
use dyndo_core::track::Track;
use serde::Deserialize;

use crate::error::ServerError;

/// The `filter` query parameter and its shorthand, as a request spells them.
///
/// Unknown parameters are refused, which is what catches an unencoded `&&`: it
/// splits the query string, so `?filter=type!=video&&height%3C=720` would otherwise
/// arrive as the perfectly valid `filter=type!=video` and be served while quietly
/// ignoring the rest of what was asked for. The junk halves reach serde as unknown
/// fields instead, and axum answers `400`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FilterQuery {
    filter: Option<String>,
    f: Option<String>,
}

impl FilterQuery {
    /// Parses whichever spelling the request used.
    pub(super) fn resolve(&self) -> Result<Option<Filter>, ServerError> {
        match (&self.filter, &self.f) {
            (Some(_), Some(_)) => Err(ServerError::InvalidFilter(
                "`filter` and `f` are the same option; pass one".to_string(),
            )),
            (Some(expression), None) | (None, Some(expression)) => Filter::parse(expression)
                .map(Some)
                .map_err(|error| ServerError::InvalidFilter(error.to_string())),
            (None, None) => Ok(None),
        }
    }
}

/// Narrows an asset to the tracks the filter keeps.
///
/// A filter that leaves at least one track serves a manifest — dropping all video
/// while keeping audio is a legitimate audio-only presentation. One that matches
/// nothing is the addressing error a `404` describes, which is the server's call to
/// make rather than the filter's.
pub(super) fn apply(
    filter: Option<&Filter>,
    asset: AssetDescriptor,
    tracks: Vec<Track>,
) -> Result<(AssetDescriptor, Vec<Track>), ServerError> {
    let Some(filter) = filter else {
        return Ok((asset, tracks));
    };

    let (asset, tracks) = filter.apply(asset, tracks);
    if asset.tracks.is_empty() {
        return Err(ServerError::FilterMatchedNothing);
    }

    Ok((asset, tracks))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(filter: Option<&str>, f: Option<&str>) -> FilterQuery {
        FilterQuery {
            filter: filter.map(str::to_string),
            f: f.map(str::to_string),
        }
    }

    #[test]
    fn resolve_accepts_either_spelling() {
        let long = query(Some("type==video"), None).resolve().unwrap();
        let short = query(None, Some("type==video")).resolve().unwrap();

        assert_eq!(long.unwrap(), short.unwrap());
    }

    #[test]
    fn resolve_without_a_filter_yields_none() {
        assert!(FilterQuery::default().resolve().unwrap().is_none());
    }

    #[test]
    fn resolve_rejects_both_spellings_at_once() {
        assert!(
            query(Some("type==video"), Some("type==audio"))
                .resolve()
                .is_err()
        );
    }

    #[test]
    fn resolve_reports_a_malformed_expression() {
        let error = query(Some("heigth<=720"), None).resolve().unwrap_err();

        assert!(
            error.to_string().contains("invalid filter"),
            "unexpected error: {error}"
        );
    }
}
