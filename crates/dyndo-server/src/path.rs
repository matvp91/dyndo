//! Safe joining of untrusted path fragments (asset ids, track sources) onto a base.

use std::path::{Component, Path, PathBuf};

use crate::error::ServerError;

/// Join `untrusted` onto `base`, rejecting anything that could escape `base`:
/// absolute paths and any non-`Normal` component (`..`, `.`, root/prefix).
pub fn resolve_within(base: &Path, untrusted: &str) -> Result<PathBuf, ServerError> {
    let rel = Path::new(untrusted);
    if rel.is_absolute() {
        return Err(ServerError::BadRequest(format!(
            "absolute path not allowed: {untrusted}"
        )));
    }
    for comp in rel.components() {
        if !matches!(comp, Component::Normal(_)) {
            return Err(ServerError::BadRequest(format!(
                "invalid path component in: {untrusted}"
            )));
        }
    }
    Ok(base.join(rel))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_normal_components() {
        let base = Path::new("/srv/assets");
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
        let base = Path::new("/srv/assets");
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
    }
}
