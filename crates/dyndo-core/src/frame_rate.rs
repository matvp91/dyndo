use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct FrameRate {
    numerator: u32,
    denominator: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ParseFrameRateError {
    #[error("invalid frame rate")]
    Invalid,
    #[error("frame rate denominator cannot be zero")]
    ZeroDenominator,
}

impl FrameRate {
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, ParseFrameRateError> {
        if denominator == 0 {
            return Err(ParseFrameRateError::ZeroDenominator);
        }

        Ok(Self {
            numerator,
            denominator,
        })
    }
}

impl fmt::Display for FrameRate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.numerator, self.denominator)
    }
}

impl FromStr for FrameRate {
    type Err = ParseFrameRateError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (numerator, denominator) = value.split_once('/').ok_or(ParseFrameRateError::Invalid)?;
        let numerator = numerator
            .parse()
            .map_err(|_| ParseFrameRateError::Invalid)?;
        let denominator = denominator
            .parse()
            .map_err(|_| ParseFrameRateError::Invalid)?;

        Self::new(numerator, denominator)
    }
}

impl From<FrameRate> for String {
    fn from(frame_rate: FrameRate) -> Self {
        frame_rate.to_string()
    }
}

impl TryFrom<String> for FrameRate {
    type Error = ParseFrameRateError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

#[cfg(test)]
mod tests {
    use super::FrameRate;

    #[test]
    fn frame_rate_should_serialize_as_a_ratio() {
        let frame_rate = FrameRate::new(25, 1).expect("frame rate should be valid");
        let value = serde_json::to_string(&frame_rate).expect("frame rate should serialize");

        assert_eq!(value, "\"25/1\"");
    }

    #[test]
    fn frame_rate_should_deserialize_from_a_ratio() {
        let frame_rate: FrameRate =
            serde_json::from_str("\"25/1\"").expect("frame rate should deserialize");

        assert_eq!(
            frame_rate,
            FrameRate::new(25, 1).expect("frame rate should be valid")
        );
    }
}
