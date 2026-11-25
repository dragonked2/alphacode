//! Secret Detection and Classification
//!
//! Provides comprehensive secret pattern matching and classification.

use super::SecretDetection;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Types of secrets we can detect and classify
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SecretType {
    /// OpenAI API keys
    OpenAIApiKey,
    /// Anthropic API keys
    AnthropicApiKey,
    /// GitHub Personal Access Tokens
    GitHubToken,
    /// GitHub App tokens
    GitHubAppToken,
    /// Google API keys
    GoogleApiKey,
    /// Google OAuth tokens
    GoogleOAuthToken,
    /// Slack tokens
    SlackToken,
    /// AWS Access Keys
    AwsAccessKey,
    /// AWS Secret Keys
    AwsSecretKey,
    /// Generic API keys (high entropy strings)
    GenericApiKey,
    /// Database connection strings
    DatabaseUrl,
    /// Private keys (SSH, PGP, etc.)
    PrivateKey,
    /// JWT tokens
    JwtToken,
    /// OAuth credentials
    OAuthCredentials,
    /// Session cookies
    SessionCookie,
    /// CI/CD secrets
    CiCdSecret,
    /// Configuration values
    ConfigValue,
    /// Browser form credentials
    BrowserCredential,
    /// Temporary auth material
    TemporaryAuth,
    /// Unknown/other
    Unknown,
}

/// A secret pattern with metadata
#[derive(Debug, Clone)]
pub struct SecretPattern {
    pub secret_type: SecretType,
    /// Compiled matcher. Not serialized: patterns are constructed in code and a
    /// `Regex` has no stable wire form. Serializing the source string instead
    /// would let a stale pattern outlive the code that defines it.
    pub pattern: Regex,
    pub confidence: f32,
    pub description: String,
}

/// Secret classifier using pattern matching
pub struct SecretClassifier {
    patterns: Vec<SecretPattern>,
    keyword_patterns: HashMap<SecretType, Vec<Regex>>,
}

impl Default for SecretClassifier {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretClassifier {
    pub fn new() -> Self {
        // Build both sets directly. The previous version assigned
        // `keyword_patterns = Self::keyword_patterns()` after initialising it to
        // an empty map, so the first binding was dead (clippy: unused
        // assignment) and the intent was obscured.
        Self {
            // Direct patterns (high confidence)
            patterns: Self::direct_patterns(),
            // Keyword-based patterns (medium confidence, context-dependent)
            keyword_patterns: Self::keyword_patterns(),
        }
    }

    fn direct_patterns() -> Vec<SecretPattern> {
        vec![
            SecretPattern {
                secret_type: SecretType::AnthropicApiKey,
                pattern: Regex::new(r"sk-ant-(?:oat|ort)01-[A-Za-z0-9_-]{20,}").unwrap(),
                confidence: 0.99,
                description: "Anthropic API key".to_string(),
            },
            SecretPattern {
                secret_type: SecretType::OpenAIApiKey,
                pattern: Regex::new(r"sk-[A-Za-z0-9]{48,}").unwrap(),
                confidence: 0.95,
                description: "OpenAI API key".to_string(),
            },
            SecretPattern {
                secret_type: SecretType::OpenAIApiKey,
                pattern: Regex::new(r"sk-proj-[A-Za-z0-9_-]{40,}").unwrap(),
                confidence: 0.99,
                description: "OpenAI project API key".to_string(),
            },
            SecretPattern {
                secret_type: SecretType::GitHubToken,
                pattern: Regex::new(r"ghp_[A-Za-z0-9]{20,}").unwrap(),
                confidence: 0.99,
                description: "GitHub Personal Access Token".to_string(),
            },
            SecretPattern {
                secret_type: SecretType::GitHubAppToken,
                pattern: Regex::new(r"github_pat_[A-Za-z0-9_]{20,}").unwrap(),
                confidence: 0.99,
                description: "GitHub App token".to_string(),
            },
            SecretPattern {
                secret_type: SecretType::GoogleApiKey,
                pattern: Regex::new(r"AIza[0-9A-Za-z_-]{20,}").unwrap(),
                confidence: 0.99,
                description: "Google API key".to_string(),
            },
            SecretPattern {
                secret_type: SecretType::GoogleOAuthToken,
                pattern: Regex::new(r"ya29\.[A-Za-z0-9._-]{20,}").unwrap(),
                confidence: 0.95,
                description: "Google OAuth token".to_string(),
            },
            SecretPattern {
                secret_type: SecretType::SlackToken,
                pattern: Regex::new(r"xox[baprs]-[A-Za-z0-9-]{10,}").unwrap(),
                confidence: 0.95,
                description: "Slack token".to_string(),
            },
            SecretPattern {
                secret_type: SecretType::AwsAccessKey,
                pattern: Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),
                confidence: 0.99,
                description: "AWS Access Key ID".to_string(),
            },
            SecretPattern {
                secret_type: SecretType::JwtToken,
                pattern: Regex::new(r"eyJ[A-Za-z0-9_-]*\.eyJ[A-Za-z0-9_-]*\.[A-Za-z0-9_-]*")
                    .unwrap(),
                confidence: 0.9,
                description: "JWT token".to_string(),
            },
            SecretPattern {
                secret_type: SecretType::AwsSecretKey,
                pattern: Regex::new(r"[A-Za-z0-9/+=]{40}").unwrap(),
                confidence: 0.7,
                description: "AWS Secret Access Key (heuristic)".to_string(),
            },
            SecretPattern {
                secret_type: SecretType::PrivateKey,
                pattern: Regex::new(r"-----BEGIN (?:RSA|DSA|EC|OPENSSH) PRIVATE KEY-----").unwrap(),
                confidence: 0.99,
                description: "Private key".to_string(),
            },
        ]
    }

    fn keyword_patterns() -> HashMap<SecretType, Vec<Regex>> {
        let mut map: HashMap<SecretType, Vec<Regex>> = HashMap::new();

        // Assignment patterns for environment variables and config
        let assignment_keys = vec![
            ("OPENROUTER_API_KEY", SecretType::GenericApiKey),
            ("OPENCODE_API_KEY", SecretType::GenericApiKey),
            ("OPENCODE_GO_API_KEY", SecretType::GenericApiKey),
            ("ZHIPU_API_KEY", SecretType::GenericApiKey),
            ("ZAI_API_KEY", SecretType::GenericApiKey),
            ("AI302_API_KEY", SecretType::GenericApiKey),
            ("BASETEN_API_KEY", SecretType::GenericApiKey),
            ("CORTECS_API_KEY", SecretType::GenericApiKey),
            ("DEEPSEEK_API_KEY", SecretType::GenericApiKey),
            ("FIRMWARE_API_KEY", SecretType::GenericApiKey),
            ("HF_TOKEN", SecretType::GenericApiKey),
            ("MOONSHOT_API_KEY", SecretType::GenericApiKey),
            ("NEBIUS_API_KEY", SecretType::GenericApiKey),
            ("SCALEWAY_API_KEY", SecretType::GenericApiKey),
            ("STACKIT_API_KEY", SecretType::GenericApiKey),
            ("GROQ_API_KEY", SecretType::GenericApiKey),
            ("MISTRAL_API_KEY", SecretType::GenericApiKey),
            ("PERPLEXITY_API_KEY", SecretType::GenericApiKey),
            ("TOGETHER_API_KEY", SecretType::GenericApiKey),
            ("DEEPINFRA_API_KEY", SecretType::GenericApiKey),
            ("XAI_API_KEY", SecretType::GenericApiKey),
            ("LMSTUDIO_API_KEY", SecretType::GenericApiKey),
            ("OLLAMA_API_KEY", SecretType::GenericApiKey),
            ("CHUTES_API_KEY", SecretType::GenericApiKey),
            ("CEREBRAS_API_KEY", SecretType::GenericApiKey),
            ("OPENAI_COMPAT_API_KEY", SecretType::OpenAIApiKey),
            ("ANTHROPIC_API_KEY", SecretType::AnthropicApiKey),
            ("OPENAI_API_KEY", SecretType::OpenAIApiKey),
            ("AZURE_OPENAI_API_KEY", SecretType::OpenAIApiKey),
            ("CURSOR_API_KEY", SecretType::GenericApiKey),
            ("GITHUB_TOKEN", SecretType::GitHubToken),
            ("DATABASE_URL", SecretType::DatabaseUrl),
            ("POSTGRES_URL", SecretType::DatabaseUrl),
            ("MYSQL_URL", SecretType::DatabaseUrl),
            ("MONGODB_URI", SecretType::DatabaseUrl),
            ("REDIS_URL", SecretType::DatabaseUrl),
            ("JWT_SECRET", SecretType::ConfigValue),
            ("SESSION_SECRET", SecretType::SessionCookie),
            ("COOKIE_SECRET", SecretType::SessionCookie),
        ];

        for (key, secret_type) in assignment_keys {
            let pattern =
                Regex::new(&format!(r"(?m)^\s*{}\s*=\s*[^\r\n]+", regex::escape(key))).unwrap();
            map.entry(secret_type).or_default().push(pattern);
        }

        map
    }

    /// Classify a single secret value
    pub fn classify(&self, value: &str) -> SecretType {
        for pattern in &self.patterns {
            if pattern.pattern.is_match(value) {
                return pattern.secret_type;
            }
        }
        SecretType::Unknown
    }

    /// Scan text for all secret occurrences
    pub fn scan(&self, text: &str) -> Vec<SecretDetection> {
        let mut detections = Vec::new();

        // Direct pattern matches
        for pattern in &self.patterns {
            for mat in pattern.pattern.find_iter(text) {
                detections.push(SecretDetection {
                    secret_type: pattern.secret_type,
                    pattern: pattern.pattern.as_str().to_string(),
                    location: mat.start(),
                    length: mat.len(),
                    confidence: pattern.confidence,
                });
            }
        }

        // Keyword-based matches (lower confidence, more context needed)
        for (secret_type, patterns) in &self.keyword_patterns {
            for pattern in patterns {
                for mat in pattern.find_iter(text) {
                    detections.push(SecretDetection {
                        secret_type: *secret_type,
                        pattern: pattern.as_str().to_string(),
                        location: mat.start(),
                        length: mat.len(),
                        confidence: 0.6, // Lower confidence for keyword-based
                    });
                }
            }
        }

        // Sort by location
        detections.sort_by_key(|d| d.location);
        detections
    }
}

/// Global patterns for easy access
pub static SECRET_PATTERNS: OnceLock<Vec<SecretPattern>> = OnceLock::new();

/// Classify a secret value using global patterns
pub fn classify_secret(value: &str) -> SecretType {
    let patterns = SECRET_PATTERNS.get_or_init(SecretClassifier::direct_patterns);
    for pattern in patterns {
        if pattern.pattern.is_match(value) {
            return pattern.secret_type;
        }
    }
    SecretType::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_anthropic_key() {
        let classifier = SecretClassifier::new();
        assert_eq!(
            classifier.classify("sk-ant-oat01-abcdefghijklmnopqrstuvwxyz"),
            SecretType::AnthropicApiKey
        );
    }

    #[test]
    fn test_classify_openai_key() {
        let classifier = SecretClassifier::new();
        assert_eq!(
            classifier.classify("sk-abcdefghijklmnopqrstuvwxyz123456789012345678901234"),
            SecretType::OpenAIApiKey
        );
    }

    #[test]
    fn test_classify_github_token() {
        let classifier = SecretClassifier::new();
        assert_eq!(
            classifier.classify("ghp_abcdefghijklmnopqrstuvwxyz123456"),
            SecretType::GitHubToken
        );
    }

    #[test]
    fn test_scan_finds_multiple_secrets() {
        let classifier = SecretClassifier::new();
        let text = "Key1: sk-ant-oat01-abcdefghijklmnopqrstuvwxyz Key2: ghp_abcdefghijklmnopqrstuvwxyz123456";
        let detections = classifier.scan(text);
        assert_eq!(detections.len(), 2);
        assert_eq!(detections[0].secret_type, SecretType::AnthropicApiKey);
        assert_eq!(detections[1].secret_type, SecretType::GitHubToken);
    }

    #[test]
    fn test_private_key_detection() {
        let classifier = SecretClassifier::new();
        let key =
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA...\n-----END RSA PRIVATE KEY-----";
        assert_eq!(classifier.classify(key), SecretType::PrivateKey);
    }

    #[test]
    fn test_jwt_detection() {
        let classifier = SecretClassifier::new();
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        assert_eq!(classifier.classify(jwt), SecretType::JwtToken);
    }
}
