use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;
use axum::{
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use url::Url;
use dyndo_core::{
    find_segment_by_time, generate_mpd, read_header, Asset, LocalFile, Source, Stream, Track,
};

use crate::config::Config;
use crate::error::ServerError;

// Join `untrusted` onto `base` via URL path resolution and reject anything that
// resolves outside `base` — traversal (`..`), absolute paths, and foreign
// schemes all fail the containment check. `base` must be an absolute path; its
// URL form keeps the trailing slash so a sibling like `assets-evil` can't pass.
fn resolve_within(base: &StdPath, untrusted: &str) -> Result<PathBuf, ServerError> {
    let base_url = Url::from_directory_path(base)
        .map_err(|_| ServerError::Internal(format!("base is not absolute: {}", base.display())))?;
    let joined = base_url
        .join(untrusted)
        .map_err(|e| ServerError::BadRequest(format!("invalid path {untrusted}: {e}")))?;
    if !joined.as_str().starts_with(base_url.as_str()) {
        return Err(ServerError::BadRequest(format!(
            "path escapes base: {untrusted}"
        )));
    }
    joined
        .to_file_path()
        .map_err(|_| ServerError::BadRequest(format!("not a file path: {untrusted}")))
}

pub(crate) fn build_router(config: Arc<Config>) -> Router {
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any);
    Router::new()
        .route("/{asset}/dash/index.mpd", get(manifest))
        .route("/{asset}/dash/{repr}/{seg}", get(segment))
        .with_state(config)
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
    State(config): State<Arc<Config>>,
    Path(asset_id): Path<String>,
) -> Result<Response, ServerError> {
    let (asset_dir, mut asset) = load_asset(&config.assets_base_path, &asset_id).await?;
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
    State(config): State<Arc<Config>>,
    Path((asset_id, repr, seg)): Path<(String, String, String)>,
) -> Result<Response, ServerError> {
    let (asset_dir, asset) = load_asset(&config.assets_base_path, &asset_id).await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_normal_components() {
        let base = StdPath::new("/srv/assets");
        assert_eq!(
            resolve_within(base, "bbb").unwrap(),
            PathBuf::from("/srv/assets/bbb")
        );
        assert_eq!(
            resolve_within(base, "bbb/video.mp4").unwrap(),
            PathBuf::from("/srv/assets/bbb/video.mp4")
        );
    }

    #[test]
    fn rejects_parent_and_absolute() {
        let base = StdPath::new("/srv/assets");
        assert!(matches!(
            resolve_within(base, "../etc/passwd"),
            Err(ServerError::BadRequest(_))
        ));
        assert!(matches!(
            resolve_within(base, "/etc/passwd"),
            Err(ServerError::BadRequest(_))
        ));
        assert!(matches!(
            resolve_within(base, "a/../../b"),
            Err(ServerError::BadRequest(_))
        ));
        // A sibling directory that shares the base as a string prefix must not pass.
        assert!(matches!(
            resolve_within(base, "../assets-evil/x"),
            Err(ServerError::BadRequest(_))
        ));
    }
}
