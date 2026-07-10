//! Handlers for the DASH routes and the asset/path plumbing they share.

use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::{
    generate_mpd, load_asset, read_header, read_init_segment, read_segment, CmafHeader, LocalFile,
};
use url::Url;

use crate::config::Config;
use crate::error::ServerError;

/// `GET /{asset}/dash/index.mpd` — the asset's DASH manifest.
pub(super) async fn manifest(
    State(config): State<Arc<Config>>,
    Path(asset_id): Path<String>,
) -> Result<Response, ServerError> {
    let asset_dir = resolve_within(&config.assets_base_path, &asset_id)?;
    let asset = load_asset(&asset_dir, &asset_id).await?;
    // generate_mpd resolves each track's source against the asset dir at read time.
    let xml = generate_mpd(&asset, &asset_dir, true).await?;
    Ok(([(CONTENT_TYPE, "application/dash+xml")], xml).into_response())
}

/// `GET /{asset}/dash/{repr}/init.mp4` — the representation's init segment.
pub(super) async fn init_segment(
    State(config): State<Arc<Config>>,
    Path((asset_id, repr)): Path<(String, String)>,
) -> Result<Response, ServerError> {
    let (source, header) = locate(&config, &asset_id, &repr).await?;
    let bytes = read_init_segment(&source, &header).await?;
    Ok(([(CONTENT_TYPE, header.stream.mime_type())], bytes).into_response())
}

/// `GET /{asset}/dash/{repr}/{seg}` — the media segment named `{time}.m4s`.
pub(super) async fn media_segment(
    State(config): State<Arc<Config>>,
    Path((asset_id, repr, seg)): Path<(String, String, String)>,
) -> Result<Response, ServerError> {
    // Resolve the representation first so an unknown asset/repr is a 404 even when
    // the segment name is also malformed.
    let (source, header) = locate(&config, &asset_id, &repr).await?;
    let time: u64 = seg
        .strip_suffix(".m4s")
        .ok_or_else(|| ServerError::NotFound(format!("unknown segment: {seg}")))?
        .parse()
        .map_err(|_| ServerError::BadRequest(format!("invalid segment time: {seg}")))?;
    let bytes = read_segment(&source, &header, time)
        .await?
        .ok_or_else(|| ServerError::NotFound(format!("no segment at time {time}")))?;
    Ok(([(CONTENT_TYPE, header.stream.mime_type())], bytes).into_response())
}

/// Resolve `{asset}/{repr}` to the representation's source file and its parsed
/// CMAF header, which carries both the segment map and the media (MIME) type.
async fn locate(
    config: &Config,
    asset_id: &str,
    repr: &str,
) -> Result<(LocalFile, CmafHeader), ServerError> {
    let asset_dir = resolve_within(&config.assets_base_path, asset_id)?;
    let asset = load_asset(&asset_dir, asset_id).await?;
    let track = asset
        .track(repr)
        .ok_or_else(|| ServerError::NotFound(format!("no representation {repr}")))?;
    let abs = resolve_within(&asset_dir, track.source())?;
    let key = abs.to_string_lossy().into_owned();
    let source = LocalFile::new(&abs);
    let header = read_header(&source, &key).await?;
    Ok((source, header))
}

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
