//! Asset resolution and CMAF segment serving shared by every streaming protocol.
//! Only the manifest differs per protocol (see [`super::dash`]); init and media
//! segments are the same CMAF bytes however they're indexed.

use std::path::{Path as StdPath, PathBuf};

use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_core::{
    load_asset, read_header, read_init_segment, read_segment, Asset, CmafHeader, LocalFile,
};
use url::Url;

use crate::error::ServerError;

/// `{repr}/init.mp4` — the representation's init segment.
pub(super) async fn init_segment(
    assets_base: &StdPath,
    asset_path: &str,
    repr: &str,
) -> Result<Response, ServerError> {
    let (source, header) = locate(assets_base, asset_path, repr).await?;
    let bytes = read_init_segment(&source, &header).await?;
    Ok(([(CONTENT_TYPE, header.stream.mime_type())], bytes).into_response())
}

/// `{repr}/{seg}` — the media segment named `{time}.m4s`.
pub(super) async fn media_segment(
    assets_base: &StdPath,
    asset_path: &str,
    repr: &str,
    seg: &str,
) -> Result<Response, ServerError> {
    // Resolve the representation first so an unknown asset/repr is a 404 even when
    // the segment name is also malformed.
    let (source, header) = locate(assets_base, asset_path, repr).await?;
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

/// Resolve `asset_path` (a descriptor JSON path relative to the base, possibly
/// nested) to the loaded [`Asset`] and the directory its track sources resolve
/// against — the descriptor's own parent directory.
pub(super) async fn load(
    assets_base: &StdPath,
    asset_path: &str,
) -> Result<(Asset, PathBuf), ServerError> {
    let asset_file = resolve_within(assets_base, asset_path)?;
    let asset_dir = asset_file
        .parent()
        .ok_or_else(|| ServerError::BadRequest(format!("asset path has no parent: {asset_path}")))?
        .to_path_buf();
    let asset = load_asset(&asset_file, asset_path).await?;
    Ok((asset, asset_dir))
}

/// Resolve `{asset_path}/{repr}` to the representation's source file and its
/// parsed CMAF header, which carries both the segment map and the media (MIME)
/// type.
async fn locate(
    assets_base: &StdPath,
    asset_path: &str,
    repr: &str,
) -> Result<(LocalFile, CmafHeader), ServerError> {
    let (asset, asset_dir) = load(assets_base, asset_path).await?;
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
        // Nested descriptor paths resolve just like any other relative path.
        assert_eq!(
            resolve_within(base, "movies/clip/asset.json").unwrap(),
            PathBuf::from("/srv/assets/movies/clip/asset.json")
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
