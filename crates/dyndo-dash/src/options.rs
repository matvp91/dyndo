use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DashOptions {
    #[serde(default, alias = "c")]
    pub compact: bool,
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
}
