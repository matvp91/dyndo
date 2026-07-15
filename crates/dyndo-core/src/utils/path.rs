//! Resolving and relativizing descriptor-relative storage paths.

use relative_path::RelativePath;

/// Resolve `file_path`, given relative to `descriptor_path`'s directory, into a
/// normalized storage path.
pub fn resolve(descriptor_path: &str, file_path: &str) -> String {
    RelativePath::new(descriptor_path)
        .parent()
        .expect("descriptor path always has a parent")
        .join(file_path)
        .normalize()
        .into_string()
}

/// Relativize a resolved `path` back to a path relative to `descriptor_path`'s
/// directory.
pub fn relativize(descriptor_path: &str, path: &str) -> String {
    RelativePath::new(descriptor_path)
        .parent()
        .expect("descriptor path always has a parent")
        .relative(path)
        .into_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_joins_a_sibling_against_the_base_dir() {
        assert_eq!(resolve("out/asset.json", "video.mp4"), "out/video.mp4");
    }

    #[test]
    fn resolve_normalizes_parent_segments() {
        assert_eq!(resolve("out/asset.json", "../video.mp4"), "video.mp4");
    }

    #[test]
    fn relativize_is_the_inverse_of_resolve_for_a_sibling() {
        let resolved = resolve("out/asset.json", "subs/eng.mp4");
        assert_eq!(relativize("out/asset.json", &resolved), "subs/eng.mp4");
    }
}
