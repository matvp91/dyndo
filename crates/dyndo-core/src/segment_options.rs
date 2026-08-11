use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SegmentOptions {
    /// The shortest a served segment may be, in milliseconds; fragments are
    /// grouped until they reach it.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub min_length: u32,
    /// How long each segment of a packaged subtitle track is, in milliseconds.
    /// Unlike `min_length` this is exact, since dyndo fragments those tracks
    /// itself rather than grouping what a file already contains. Zero asks for no
    /// grid, leaving the asset's splice points as the only cuts.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub text_length: u32,
    /// Times a segment has to start at, in milliseconds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boundaries: Vec<u32>,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

#[cfg(test)]
mod tests {
    use super::SegmentOptions;

    #[test]
    fn serialization_omits_default_options() {
        let serialized = serde_json::to_value(SegmentOptions::default()).unwrap();

        assert_eq!(serialized, serde_json::json!({}));
    }

    #[test]
    fn serialization_preserves_non_default_options() {
        let options = SegmentOptions {
            min_length: 1_000,
            text_length: 2_000,
            boundaries: vec![3_000],
        };

        let serialized = serde_json::to_value(options).unwrap();

        assert_eq!(
            serialized,
            serde_json::json!({
                "min_length": 1_000,
                "text_length": 2_000,
                "boundaries": [3_000],
            })
        );
    }
}
