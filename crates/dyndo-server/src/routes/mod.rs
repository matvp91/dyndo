//! Route table and protocol dispatch. Every stream lives under
//! `/{asset_path}/{protocol}/…`, where `{asset_path}` is a (possibly nested)
//! descriptor JSON path relative to the assets base and `{protocol}` is one of
//! [`PROTOCOLS`]. The descriptor path is variable-length and sits *before* the
//! fixed protocol infix, which no declarative route can express — so the whole
//! tail is captured by one catch-all and split by [`split_route`].

mod segment;
mod transport;

use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
    routing::get,
};
use dyndo_core::asset::Asset;
use dyndo_core::track::Track;
use opendal::Operator;
use tower_http::cors::{Any, CorsLayer};

use crate::error::ServerError;

/// Streaming protocols we recognise. The manifest resource differs per protocol
/// (`index.mpd` for DASH, `index.m3u8` + `{repr}.m3u8` for HLS); media segments
/// are the same CMAF for all.
const PROTOCOLS: &[&str] = &["dash", "hls"];

pub(crate) fn build_router(op: Operator) -> Router {
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any);
    Router::new()
        .route("/health", get(health))
        .route("/{*path}", get(dispatch))
        .with_state(op)
        .layer(cors)
}

/// Liveness probe. Static routes win over the catch-all, so this never shadows
/// a streaming route (those always carry a `/{protocol}/` infix).
async fn health() -> StatusCode {
    StatusCode::OK
}

/// Route the catch-all tail to the right handler.
async fn dispatch(
    State(op): State<Operator>,
    Path(path): Path<String>,
) -> Result<Response, ServerError> {
    let path = path.strip_prefix('/').unwrap_or(path.as_str());
    let (asset_path, protocol, resource) = split_route(path)
        .ok_or_else(|| ServerError::NotFound(format!("not a streaming route: {path}")))?;

    // A resource with a slash is `{repr}/…` — a media segment, served straight
    // from CMAF and identical for every protocol. A bare resource is the
    // manifest, which each protocol renders its own way.
    match resource.split_once('/') {
        Some((repr, "init.mp4")) => segment::init_segment(&op, asset_path, repr).await,
        Some((repr, seg)) => segment::media_segment(&op, asset_path, repr, seg).await,
        None => match (protocol, resource) {
            ("dash", "index.mpd") => transport::dash_manifest(&op, asset_path).await,
            ("hls", "index.m3u8") => transport::hls_master(&op, asset_path).await,
            ("hls", r) if r.ends_with(".m3u8") => {
                let repr = r
                    .strip_suffix(".m3u8")
                    .expect("guarded by the .ends_with(\".m3u8\") arm");
                transport::hls_media(&op, asset_path, repr).await
            }
            _ => Err(ServerError::NotFound(format!(
                "no {protocol} resource {resource}"
            ))),
        },
    }
}

/// Split `{asset_path}/{protocol}/{resource}` on the *rightmost* `/{protocol}/`
/// for a known protocol, so a descriptor directory named after a protocol still
/// resolves. Returns `None` when no known protocol delimiter is present.
fn split_route(path: &str) -> Option<(&str, &str, &str)> {
    let mut best: Option<(usize, &str, usize)> = None;
    for proto in PROTOCOLS.iter().copied() {
        let delim = format!("/{proto}/");
        if let Some(i) = path.rfind(&delim)
            && best.is_none_or(|(bi, _, _)| i > bi)
        {
            best = Some((i, proto, i + delim.len()));
        }
    }
    best.map(|(i, proto, res_start)| (&path[..i], proto, &path[res_start..]))
}

/// Find the asset track whose representation id is `repr`. 404 if none
/// matches. Raw (non-CMAF) tracks resolve too; their segment accessors
/// answer with empty data (no init segment, no media segments).
fn find_track<'a>(asset: &'a Asset, repr: &str) -> Result<&'a Track, ServerError> {
    asset
        .tracks
        .iter()
        .find(|t| t.id == repr)
        .ok_or_else(|| ServerError::NotFound(format!("no representation {repr}")))
}
