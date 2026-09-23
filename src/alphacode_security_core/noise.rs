use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// OPSEC noise level for security operations.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NoiseLevel {
    Quiet,
    #[default]
    Moderate,
    Loud,
}

impl NoiseLevel {
    pub fn as_str(&self) -> Cow<'_, str> {
        match self {
            Self::Quiet => Cow::Borrowed("quiet"),
            Self::Moderate => Cow::Borrowed("moderate"),
            Self::Loud => Cow::Borrowed("loud"),
        }
    }

    pub fn risk_factor(&self) -> f64 {
        match self {
            Self::Quiet => 0.1,
            Self::Moderate => 0.5,
            Self::Loud => 0.9,
        }
    }
}

impl From<String> for NoiseLevel {
    fn from(value: String) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "quiet" | "q" => Self::Quiet,
            "moderate" | "mod" | "m" => Self::Moderate,
            "loud" | "l" => Self::Loud,
            _ => Self::Moderate,
        }
    }
}

impl From<&str> for NoiseLevel {
    fn from(value: &str) -> Self {
        Self::from(value.to_string())
    }
}

impl Serialize for NoiseLevel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str().as_ref())
    }
}

impl<'de> Deserialize<'de> for NoiseLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self::from(String::deserialize(deserializer)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_level_ordering() {
        assert!(NoiseLevel::Quiet < NoiseLevel::Moderate);
        assert!(NoiseLevel::Moderate < NoiseLevel::Loud);
    }

    #[test]
    fn noise_level_risk_factors() {
        assert!((NoiseLevel::Quiet.risk_factor() - 0.1).abs() < f64::EPSILON);
        assert!((NoiseLevel::Moderate.risk_factor() - 0.5).abs() < f64::EPSILON);
        assert!((NoiseLevel::Loud.risk_factor() - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn noise_level_from_string() {
        assert_eq!(NoiseLevel::from("quiet"), NoiseLevel::Quiet);
        assert_eq!(NoiseLevel::from("moderate"), NoiseLevel::Moderate);
        assert_eq!(NoiseLevel::from("LOUD"), NoiseLevel::Loud);
        assert_eq!(NoiseLevel::from("unknown"), NoiseLevel::Moderate);
    }
}
