use axum::{
    Router,
    extract::{Path, Query, State},
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
    routing::get,
};
use dyndo_core::asset::ResolvedAsset;
use dyndo_core::track::ResolvedTrack;
use dyndo_dash::options::DashOptions;
use dyndo_hls::options::HlsOptions;
use opendal::Operator;
use serde::Deserialize;

use super::filter::Filter;
use super::options::Options;
use super::{load_asset, resolve_asset, resolve_track};
use crate::error::ServerError;

const DASH_CONTENT_TYPE: &str = "application/dash+xml";
const HLS_CONTENT_TYPE: &str = "application/vnd.apple.mpegurl";

pub(super) trait ManifestRoute {
    /// Registers the DASH and HLS manifest route.
    fn manifest_route(self) -> Self;

    /// Serves the requested DASH manifest or HLS playlist.
    async fn manifest(
        op: State<Operator>,
        path: Path<(String, String)>,
        query: Query<ManifestQuery>,
    ) -> Result<Response, ServerError>;
}

// Rejecting unknown fields prevents an unencoded `&&` from silently truncating a filter.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestQuery {
    filter: Option<Filter>,
}

impl ManifestRoute for Router<Operator> {
    fn manifest_route(self) -> Self {
        self.route("/out/{options}/{resource}", get(Self::manifest))
    }

    async fn manifest(
        State(op): State<Operator>,
        Path((encoded_options, resource)): Path<(String, String)>,
        Query(query): Query<ManifestQuery>,
    ) -> Result<Response, ServerError> {
        let not_found = || ServerError::NotFound(resource.clone());
        let resource = resource.rsplit_once('.').ok_or_else(not_found)?;
        let options = Options::parse(&encoded_options)?;
        let asset = load_asset(&op, &options).await?;

        match resource {
            ("index", "mpd") => {
                let resolved = resolve_asset(&op, &asset, query.filter.as_ref()).await?;
                let dash_options = options.dash_options();
                dash(
                    &resolved,
                    options.min_length(),
                    options.text_length(),
                    &dash_options,
                )
                .await
            }
            ("master", "m3u8") => {
                let resolved = resolve_asset(&op, &asset, query.filter.as_ref()).await?;
                let hls_options = options.hls_options();
                hls_master(
                    &resolved,
                    options.min_length(),
                    options.text_length(),
                    &hls_options,
                )
                .await
            }
            (resource, "m3u8") => {
                let hls_options = options.hls_options();
                let track = resolve_track(&op, &asset, resource, query.filter.as_ref()).await?;
                if track.thumbnail().is_some() {
                    hls_images(&track).await
                } else {
                    hls_media(
                        &track,
                        options.min_length(),
                        options.text_length(),
                        &asset.boundaries,
                        &hls_options,
                    )
                    .await
                }
            }
            _ => Err(not_found()),
        }
    }
}

pub(super) async fn dash(
    asset: &ResolvedAsset,
    min_length: u32,
    text_length: u32,
    options: &DashOptions,
) -> Result<Response, ServerError> {
    let xml = dyndo_dash::generate_mpd(asset, min_length, text_length, options).await?;
    Ok(([(CONTENT_TYPE, DASH_CONTENT_TYPE)], xml).into_response())
}

pub(super) async fn hls_master(
    asset: &ResolvedAsset,
    min_length: u32,
    text_length: u32,
    options: &HlsOptions,
) -> Result<Response, ServerError> {
    let playlist =
        dyndo_hls::generate_master_playlist(asset, min_length, text_length, options).await?;
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist).into_response())
}

pub(super) async fn hls_media(
    track: &ResolvedTrack,
    min_length: u32,
    text_length: u32,
    boundaries: &[u32],
    options: &HlsOptions,
) -> Result<Response, ServerError> {
    let playlist =
        dyndo_hls::generate_media_playlist(track, min_length, text_length, boundaries, options)
            .await?;
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist).into_response())
}

pub(super) async fn hls_images(track: &ResolvedTrack) -> Result<Response, ServerError> {
    let thumbnail = track
        .thumbnail()
        .ok_or_else(|| ServerError::NotFound(format!("thumbnail {}", track.id())))?;
    let playlist = dyndo_hls::generate_image_playlist(thumbnail)?;
    Ok(([(CONTENT_TYPE, HLS_CONTENT_TYPE)], playlist).into_response())
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use axum::http::Uri;

    use super::ManifestQuery;

    #[test]
    fn manifest_query_deserializes_filter() {
        let uri: Uri = "/?filter=type%3D%3Dvideo".parse().unwrap();
        let Query(query) = Query::<ManifestQuery>::try_from_uri(&uri).unwrap();

        assert!(query.filter.is_some());
    }

    #[test]
    fn manifest_query_rejects_invalid_filter() {
        let uri: Uri = "/?filter=type%3Evideo".parse().unwrap();

        assert!(Query::<ManifestQuery>::try_from_uri(&uri).is_err());
    }
}
