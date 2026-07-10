//! Route table and protocol dispatch. Every stream lives under
//! `/{asset_path}/{protocol}/…`, where `{asset_path}` is a (possibly nested)
//! descriptor JSON path relative to the assets base and `{protocol}` is one of
//! [`PROTOCOLS`]. The descriptor path is variable-length and sits *before* the
//! fixed protocol infix, which no declarative route can express — so the whole
//! tail is captured by one catch-all and split by [`split_route`].

mod dash;

use axum::{
    extract::{Path, State},
    response::Response,
    routing::get,
    Router,
};
use opendal::Operator;
use tower_http::cors::{Any, CorsLayer};

use crate::error::ServerError;

/// Streaming protocols we recognise. The manifest resource differs per protocol
/// (`index.mpd` for DASH); media segments are the same CMAF for all. To light up
/// HLS, add `"hls"` here and a sibling manifest arm in [`dispatch`].
const PROTOCOLS: &[&str] = &["dash"];

pub(crate) fn build_router(op: Operator) -> Router {
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any);
    Router::new()
        .route("/{*path}", get(dispatch))
        .with_state(op)
        .layer(cors)
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
        Some((repr, "init.mp4")) => dash::init_segment(&op, asset_path, repr).await,
        Some((repr, seg)) => dash::media_segment(&op, asset_path, repr, seg).await,
        None => match (protocol, resource) {
            ("dash", "index.mpd") => dash::manifest(&op, asset_path).await,
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
        if let Some(i) = path.rfind(&delim) {
            if best.is_none_or(|(bi, _, _)| i > bi) {
                best = Some((i, proto, i + delim.len()));
            }
        }
    }
    best.map(|(i, proto, res_start)| (&path[..i], proto, &path[res_start..]))
}
