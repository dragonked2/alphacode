use serde::{Deserialize, Serialize};

/// The operating mode of the Decision Plane.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum DecisionMode {
    /// Decision Plane is completely disabled. Existing AlphaCode behavior preserved.
    Off,
    /// Decision Plane runs but cannot influence behavior. Records only.
    #[default]
    Observe,
    /// Decision Plane makes decisions and records what it WOULD have done,
    /// but does not alter execution. Used for A/B comparison.
    Shadow,
    /// Decision Plane is fully active and can influence approved decision points.
    Active,
}

impl DecisionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Observe => "observe",
            Self::Shadow => "shadow",
            Self::Active => "active",
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "observe" => Some(Self::Observe),
            "shadow" => Some(Self::Shadow),
            "active" => Some(Self::Active),
            _ => None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        !matches!(self, Self::Off)
    }

    pub fn can_influence(&self) -> bool {
        matches!(self, Self::Active)
    }
}

/// Confidence thresholds for decision routing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionThresholds {
    /// Below this confidence, the engine abstains entirely.
    pub abstain: f32,
    /// Below this confidence, defer to generative reasoning.
    pub escalate: f32,
    /// Above this confidence, proceed with high confidence.
    pub high_confidence: f32,
}

impl Default for DecisionThresholds {
    fn default() -> Self {
        Self {
            abstain: 0.30,
            escalate: 0.60,
            high_confidence: 0.90,
        }
    }
}

/// Configuration for the Decision Plane.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionConfig {
    /// The operating mode.
    pub mode: DecisionMode,
    /// Which provider to use for decisions ("auto" uses the primary provider).
    pub provider: String,
    /// Which model to use for decisions ("auto" uses the primary model).
    pub model: String,
    /// Whether to fall back to generative reasoning on decision failure.
    pub fallback_enabled: bool,
    /// Whether to allow parallel decision requests.
    pub parallel: bool,
    /// Whether to cache decisions.
    pub cache_enabled: bool,
    /// Whether to record telemetry.
    pub telemetry_enabled: bool,
    /// Confidence thresholds.
    pub thresholds: DecisionThresholds,
    /// Maximum number of cached decisions (0 = unlimited).
    pub cache_max_entries: usize,
    /// Cache TTL in seconds (0 = no expiry).
    pub cache_ttl_secs: u64,
}

impl Default for DecisionConfig {
    fn default() -> Self {
        Self {
            mode: DecisionMode::default(),
            provider: "auto".to_string(),
            model: "auto".to_string(),
            fallback_enabled: true,
            parallel: true,
            cache_enabled: true,
            telemetry_enabled: true,
            thresholds: DecisionThresholds::default(),
            cache_max_entries: 1024,
            cache_ttl_secs: 300,
        }
    }
}

impl DecisionConfig {
    pub fn off() -> Self {
        Self {
            mode: DecisionMode::Off,
            ..Default::default()
        }
    }

    pub fn active() -> Self {
        Self {
            mode: DecisionMode::Active,
            ..Default::default()
        }
    }

    pub fn shadow() -> Self {
        Self {
            mode: DecisionMode::Shadow,
            ..Default::default()
        }
    }

    pub fn observe() -> Self {
        Self {
            mode: DecisionMode::Observe,
            ..Default::default()
        }
    }

    /// Convert from the config-types DecisionPlaneConfig.
    pub fn from_config_types(cfg: &crate::alphacode_config_types::DecisionPlaneConfig) -> Self {
        Self {
            mode: match cfg.mode {
                crate::alphacode_config_types::DecisionPlaneMode::Off => DecisionMode::Off,
                crate::alphacode_config_types::DecisionPlaneMode::Observe => DecisionMode::Observe,
                crate::alphacode_config_types::DecisionPlaneMode::Shadow => DecisionMode::Shadow,
                crate::alphacode_config_types::DecisionPlaneMode::Active => DecisionMode::Active,
            },
            provider: cfg.provider.clone(),
            model: cfg.model.clone(),
            fallback_enabled: cfg.fallback_enabled,
            parallel: true,
            cache_enabled: cfg.cache_enabled,
            telemetry_enabled: cfg.telemetry_enabled,
            thresholds: DecisionThresholds::default(),
            cache_max_entries: cfg.cache_max_entries,
            cache_ttl_secs: cfg.cache_ttl_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_mode_parse_roundtrip() {
        assert_eq!(DecisionMode::parse("off"), Some(DecisionMode::Off));
        assert_eq!(DecisionMode::parse("OBSERVE"), Some(DecisionMode::Observe));
        assert_eq!(DecisionMode::parse("Shadow"), Some(DecisionMode::Shadow));
        assert_eq!(DecisionMode::parse("active"), Some(DecisionMode::Active));
        assert_eq!(DecisionMode::parse("invalid"), None);
    }

    #[test]
    fn decision_mode_semantics() {
        assert!(!DecisionMode::Off.is_enabled());
        assert!(DecisionMode::Observe.is_enabled());
        assert!(DecisionMode::Shadow.is_enabled());
        assert!(DecisionMode::Active.is_enabled());

        assert!(!DecisionMode::Off.can_influence());
        assert!(!DecisionMode::Observe.can_influence());
        assert!(!DecisionMode::Shadow.can_influence());
        assert!(DecisionMode::Active.can_influence());
    }

    #[test]
    fn default_config_is_observe() {
        let c = DecisionConfig::default();
        assert_eq!(c.mode, DecisionMode::Observe);
        assert!(c.fallback_enabled);
        assert!(c.cache_enabled);
        assert!(c.telemetry_enabled);
    }

    #[test]
    fn thresholds_are_ordered() {
        let t = DecisionThresholds::default();
        assert!(t.abstain < t.escalate);
        assert!(t.escalate < t.high_confidence);
    }
}
