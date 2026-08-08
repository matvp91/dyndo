use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
}
