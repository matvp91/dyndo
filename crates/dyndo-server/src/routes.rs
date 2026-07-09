use std::path::{Path as StdPath, PathBuf};
use axum::{
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use dyndo_core::{
    find_segment_by_time, generate_mpd, read_header, Asset, LocalFile, Source, Stream, Track,
};

use crate::error::ServerError;
use crate::path::resolve_within;
use crate::state::AppState;

pub(crate) fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any);
    Router::new()
        .route("/{asset}/dash/index.mpd", get(manifest))
        .route("/{asset}/dash/{repr}/{seg}", get(segment))
        .with_state(state)
        .layer(cors)
}

/// Load `{base}/{asset}/asset.json` into an owned `Asset`, returning the resolved
/// asset directory alongside it. Missing file -> 404; malformed JSON -> 500.
async fn load_asset(base: &StdPath, asset_id: &str) -> Result<(PathBuf, Asset), ServerError> {
    let asset_dir = resolve_within(base, asset_id)?;
    let json_path = asset_dir.join("asset.json");
    let bytes = tokio::fs::read(&json_path)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                ServerError::NotFound(format!("asset not found: {asset_id}"))
            }
            _ => ServerError::Internal(format!("reading {}: {e}", json_path.display())),
        })?;
    let asset: Asset = serde_json::from_slice(&bytes)
        .map_err(|e| ServerError::Internal(format!("invalid asset.json for {asset_id}: {e}")))?;
    Ok((asset_dir, asset))
}

async fn manifest(
    State(state): State<AppState>,
    Path(asset_id): Path<String>,
) -> Result<Response, ServerError> {
    let (asset_dir, mut asset) = load_asset(&state.assets_base_path, &asset_id).await?;
    // Rewrite each track's source to an absolute path inside the asset dir, so
    // generate_mpd (which opens the sources itself) resolves them correctly.
    for track in &mut asset.tracks {
        let rel = track.source().to_string();
        let abs = resolve_within(&asset_dir, &rel)?
            .to_string_lossy()
            .into_owned();
        match track {
            Track::Video(v) => v.source = abs,
            Track::Audio(a) => a.source = abs,
        }
    }
    let xml = generate_mpd(&asset, true).await?;
    Ok(([(header::CONTENT_TYPE, "application/dash+xml")], xml).into_response())
}

async fn segment(
    State(state): State<AppState>,
    Path((asset_id, repr, seg)): Path<(String, String, String)>,
) -> Result<Response, ServerError> {
    let (asset_dir, asset) = load_asset(&state.assets_base_path, &asset_id).await?;
    let track = asset
        .tracks
        .iter()
        .find(|t| t.id() == repr)
        .ok_or_else(|| ServerError::NotFound(format!("no representation {repr}")))?;
    let abs = resolve_within(&asset_dir, track.source())?;
    let abs_str = abs.to_string_lossy().into_owned();
    let source = LocalFile::new(&abs);
    let header = read_header(&source, &abs_str).await?;

    let content_type = match &header.stream {
        Stream::Video(_) => "video/mp4",
        Stream::Audio(_) => "audio/mp4",
    };

    let (start, len) = if seg == "init.mp4" {
        (
            header.init_range.start,
            (header.init_range.end - header.init_range.start) as usize,
        )
    } else if let Some(time_str) = seg.strip_suffix(".m4s") {
        let time: u64 = time_str
            .parse()
            .map_err(|_| ServerError::BadRequest(format!("invalid segment time: {seg}")))?;
        let range = find_segment_by_time(&header, time)
            .ok_or_else(|| ServerError::NotFound(format!("no segment at time {time}")))?;
        (range.start, (range.end - range.start) as usize)
    } else {
        return Err(ServerError::NotFound(format!("unknown segment: {seg}")));
    };

    let bytes = source.read_at(start, len).await?;
    Ok(([(header::CONTENT_TYPE, content_type)], bytes).into_response())
}
