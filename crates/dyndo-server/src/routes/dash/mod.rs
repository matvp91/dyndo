//! Handlers for the DASH routes and the asset/path plumbing they share.

mod manifest;
mod segment;

use std::path::{Path as StdPath, PathBuf};

pub(super) use manifest::manifest;
pub(super) use segment::{init_segment, media_segment};
use url::Url;

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
