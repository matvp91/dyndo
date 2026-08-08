use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DashOptions {
    #[serde(default, alias = "c")]
    pub compact: bool,
    /// Split the manifest into a Period at each segment boundary.
    ///
    /// Off by default: a boundary only asks for a segment to start there, which
    /// says nothing about whether a client should treat what follows as a
    /// separate presentation.
    #[serde(default, alias = "mp")]
    pub multi_period: bool,
    /// Thumbnails per sprite row and column. Zero describes no thumbnail track.
    #[serde(alias = "tts")]
    pub thumbnail_tile_size: u32,
    /// Height of a whole sprite, in pixels.
    #[serde(alias = "th")]
    pub thumbnail_height: u32,
    /// Milliseconds between one thumbnail and the next.
    #[serde(alias = "ts")]
    pub thumbnail_step: u32,
}

impl Default for DashOptions {
    fn default() -> Self {
        Self {
            compact: false,
            multi_period: false,
            thumbnail_tile_size: 0,
            thumbnail_height: 900,
            thumbnail_step: 10_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_defaults_to_false() {
        assert!(!DashOptions::default().compact);
    }

    #[test]
    fn compact_accepts_shorthand() {
        let options: DashOptions = serde_json::from_str(r#"{"c":true}"#).unwrap();

        assert!(options.compact);
    }

    #[test]
    fn multi_period_defaults_to_false() {
        assert!(!DashOptions::default().multi_period);
    }

    #[test]
    fn multi_period_accepts_shorthand() {
        let options: DashOptions = serde_json::from_str(r#"{"mp":true}"#).unwrap();

        assert!(options.multi_period);
    }

    #[test]
    fn thumbnails_are_off_by_default_and_sized_anyway() {
        let options = DashOptions::default();

        assert_eq!(
            (
                options.thumbnail_tile_size,
                options.thumbnail_height,
                options.thumbnail_step
            ),
            (0, 900, 10_000)
        );
    }

    #[test]
    fn thumbnails_accept_shorthands() {
        let options: DashOptions = serde_json::from_str(r#"{"tts":4,"th":720,"ts":5000}"#).unwrap();

        assert_eq!(
            (
                options.thumbnail_tile_size,
                options.thumbnail_height,
                options.thumbnail_step
            ),
            (4, 720, 5_000)
        );
    }

    #[test]
    fn a_tile_size_alone_leaves_the_sprite_at_its_default_shape() {
        let options: DashOptions = serde_json::from_str(r#"{"tts":5}"#).unwrap();

        assert_eq!(
            (options.thumbnail_height, options.thumbnail_step),
            (900, 10_000)
        );
    }
}
