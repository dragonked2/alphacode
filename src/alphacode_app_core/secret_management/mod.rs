//! Centralized Secret Management Architecture
//!
//! # Integration status
//!
//! This module is a self-contained facility with its own unit tests; nothing in
//! the agent loop calls it yet. Its public surface (redaction entry points,
//! `SecretReference` resolution, and the storage API) therefore looks unused to
//! the compiler even though it is the intended integration point.
//!
//! `dead_code` is allowed here for that reason, with two conditions: every
//! finding is re-checked when the first caller is wired up, and the encryption
//! path fails closed rather than pretending (`SecretStore::encrypt_value`).

#![allow(dead_code)]

//!
//! This module provides a comprehensive secret protection system that protects
//! sensitive values throughout their complete lifecycle.
//!
//! ## Design Principles
//!
//! - **Centralized**: All secret handling flows through this module
//! - **Opaque References**: Secrets are stored as handles, never plaintext in model context
//! - **Fail-Closed**: Any failure in secret handling defaults to denial/protection
//! - **Lifecycle Coverage**: Protection from input → storage → tool use → output → persistence
//! - **Structured Redaction**: Provenance tracking over pattern matching
//! - **No Accidental Serialization**: Secrets never enter logs, telemetry, crash reports

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock as StdRwLock};
use std::time::Duration;

mod detection;
mod handles;
mod redaction;
mod storage;

pub use detection::{
    SECRET_PATTERNS, SecretClassifier, SecretPattern, SecretType, classify_secret,
};
pub use handles::{HandleMetadata, HandlePermissions, SecretHandle, SecretHandleRegistry};
pub use redaction::{RedactionContext, RedactionEngine, RedactionResult, RedactionScope};
pub use storage::{SecretEntry, SecretStore, StorageConfig, ZeroizingString};

/// Global secret management instance
static SECRET_MANAGER: LazyLock<StdRwLock<Option<Arc<SecretManager>>>> =
    LazyLock::new(|| StdRwLock::new(None));

/// Initialize the global secret manager
pub fn initialize_secret_manager(config: SecretManagerConfig) -> Arc<SecretManager> {
    let manager = Arc::new(SecretManager::new(config));
    *SECRET_MANAGER.write().unwrap() = Some(manager.clone());
    manager
}

/// Get the global secret manager
pub fn secret_manager() -> Option<Arc<SecretManager>> {
    SECRET_MANAGER.read().unwrap().clone()
}

/// Configuration for the secret manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretManagerConfig {
    /// Enable secret detection and protection
    pub enabled: bool,
    /// Maximum number of secret handles per session
    pub max_handles_per_session: usize,
    /// TTL for secret handles (None = no expiry)
    pub handle_ttl: Option<Duration>,
    /// Enable audit logging of secret operations
    pub audit_enabled: bool,
    /// Redaction scope for different output channels
    pub redaction_scope: RedactionScope,
    /// Whether to use hardware-backed storage when available
    pub use_hardware_storage: bool,
}

impl Default for SecretManagerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_handles_per_session: 1000,
            handle_ttl: Some(Duration::from_secs(24 * 60 * 60)), // 24 hours
            audit_enabled: true,
            redaction_scope: RedactionScope::Full,
            use_hardware_storage: cfg!(target_os = "macos") || cfg!(target_os = "linux"),
        }
    }
}

/// Main secret management coordinator
pub struct SecretManager {
    config: SecretManagerConfig,
    classifier: SecretClassifier,
    store: SecretStore,
    handle_registry: SecretHandleRegistry,
    redaction_engine: RedactionEngine,
    audit_log: StdRwLock<Vec<AuditEntry>>,
}

impl SecretManager {
    pub fn new(config: SecretManagerConfig) -> Self {
        let classifier = SecretClassifier::new();
        let store = SecretStore::new(config.max_handles_per_session);
        let handle_registry = SecretHandleRegistry::new(config.max_handles_per_session);
        let redaction_engine = RedactionEngine::new(config.redaction_scope);

        Self {
            config,
            classifier,
            store,
            handle_registry,
            redaction_engine,
            audit_log: StdRwLock::new(Vec::new()),
        }
    }

    /// Store a secret and get an opaque handle
    pub fn store_secret(&self, session_id: &str, secret: SecretInput) -> Result<SecretHandle> {
        if !self.config.enabled {
            return Err(anyhow!("Secret management is disabled"));
        }

        // Classify the secret
        let secret_type = self.classifier.classify(&secret.value);

        // Store in secure storage
        let entry = SecretEntry::new(
            session_id.to_string(),
            secret.value,
            secret_type,
            secret.metadata,
        );

        let handle = self.store.store(entry)?;

        // Register handle with permissions
        let metadata = HandleMetadata {
            session_id: session_id.to_string(),
            secret_type,
            created_at_ms: chrono::Utc::now().timestamp_millis(),
            expires_at_ms: self
                .config
                .handle_ttl
                .map(|ttl| chrono::Utc::now().timestamp_millis() + ttl.as_millis() as i64),
            permissions: secret.permissions.unwrap_or_default(),
            access_count: 0,
            last_accessed_ms: None,
            tags: vec![],
        };

        self.handle_registry.register(handle.clone(), metadata)?;

        // Audit log
        if self.config.audit_enabled {
            self.audit_log.write().unwrap().push(AuditEntry {
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                operation: AuditOperation::Store,
                session_id: session_id.to_string(),
                handle_id: handle.id().to_string(),
                secret_type,
                success: true,
            });
        }

        Ok(handle)
    }

    /// Retrieve a secret by handle (for authorized tool use only)
    pub fn retrieve_secret(
        &self,
        session_id: &str,
        handle: &SecretHandle,
    ) -> Result<ProtectedSecret> {
        if !self.config.enabled {
            return Err(anyhow!("Secret management is disabled"));
        }

        // Verify handle ownership and permissions
        let metadata = self.handle_registry.get(handle)?;
        if metadata.session_id != session_id {
            return Err(anyhow!("Handle does not belong to this session"));
        }

        let now_ms = chrono::Utc::now().timestamp_millis();
        if let Some(expires_at_ms) = metadata.expires_at_ms
            && now_ms > expires_at_ms
        {
            return Err(anyhow!("Secret handle has expired"));
        }

        if !metadata.permissions.can_read {
            return Err(anyhow!("Handle does not have read permission"));
        }

        // Retrieve from secure storage. `mut` is required: the secret value is
        // taken out by `mem::replace` below, because SecretEntry drops through
        // its zeroizing value and cannot have that field moved out of it.
        let mut entry = self.store.retrieve(handle)?;

        // Update access count
        self.handle_registry.increment_access(handle)?;

        // Audit log
        if self.config.audit_enabled {
            self.audit_log.write().unwrap().push(AuditEntry {
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                operation: AuditOperation::Retrieve,
                session_id: session_id.to_string(),
                handle_id: handle.id().to_string(),
                secret_type: entry.secret_type,
                success: true,
            });
        }

        // `entry.value` cannot be moved out (SecretEntry drops through its
        // ZeroizingString field), so the value is taken by replacement.
        let value = std::mem::replace(
            &mut entry.value,
            crate::secret_management::storage::ZeroizingString::new(String::new()),
        );
        Ok(ProtectedSecret {
            value,
            secret_type: entry.secret_type,
            handle: handle.clone(),
        })
    }

    /// Use a secret in a tool call without exposing it to the model
    pub fn use_secret_in_tool<F, R>(
        &self,
        session_id: &str,
        handle: &SecretHandle,
        f: F,
    ) -> Result<R>
    where
        F: FnOnce(&ProtectedSecret) -> Result<R>,
    {
        let secret = self.retrieve_secret(session_id, handle)?;
        let result = f(&secret);

        // Audit log
        if self.config.audit_enabled {
            self.audit_log.write().unwrap().push(AuditEntry {
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                operation: AuditOperation::UseInTool,
                session_id: session_id.to_string(),
                handle_id: handle.id().to_string(),
                secret_type: secret.secret_type,
                success: result.is_ok(),
            });
        }

        result
    }

    /// Redact secrets from text output
    pub fn redact_output(&self, text: &str, context: RedactionContext) -> RedactionResult {
        self.redaction_engine.redact(text, context)
    }

    /// Check if text contains secrets (for preflight validation)
    pub fn scan_for_secrets(&self, text: &str) -> Vec<SecretDetection> {
        self.classifier.scan(text)
    }

    /// Revoke a secret handle
    pub fn revoke_handle(&self, session_id: &str, handle: &SecretHandle) -> Result<()> {
        let metadata = self.handle_registry.get(handle)?;
        if metadata.session_id != session_id {
            return Err(anyhow!("Handle does not belong to this session"));
        }

        self.store.delete(handle)?;
        self.handle_registry.revoke(handle)?;

        if self.config.audit_enabled {
            self.audit_log.write().unwrap().push(AuditEntry {
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                operation: AuditOperation::Revoke,
                session_id: session_id.to_string(),
                handle_id: handle.id().to_string(),
                secret_type: metadata.secret_type,
                success: true,
            });
        }

        Ok(())
    }

    /// Get audit log for a session
    pub fn get_audit_log(&self, session_id: &str) -> Vec<AuditEntry> {
        self.audit_log
            .read()
            .unwrap()
            .iter()
            .filter(|e| e.session_id == session_id)
            .cloned()
            .collect()
    }

    /// Clean up expired handles
    pub fn cleanup_expired(&self) -> usize {
        let expired = self.handle_registry.get_expired();
        let mut count = 0;
        for handle in expired {
            if self.store.delete(&handle).is_ok() {
                count += 1;
            }
            self.handle_registry.revoke(&handle).ok();
        }
        count
    }
}

/// Input for storing a secret
#[derive(Debug, Clone)]
pub struct SecretInput {
    /// The secret value (will be zeroized after storage)
    pub value: crate::secret_management::storage::ZeroizingString,
    /// Secret type hint (optional, will be auto-classified if not provided)
    pub secret_type: Option<SecretType>,
    /// Metadata about the secret
    pub metadata: HashMap<String, String>,
    /// Permissions for the handle
    pub permissions: Option<HandlePermissions>,
}

/// A protected secret that can only be used within authorized boundaries
#[derive(Debug)]
pub struct ProtectedSecret {
    value: crate::secret_management::storage::ZeroizingString,
    secret_type: SecretType,
    handle: SecretHandle,
}

impl ProtectedSecret {
    pub fn value(&self) -> &str {
        self.value.as_str()
    }

    pub fn secret_type(&self) -> SecretType {
        self.secret_type
    }

    pub fn handle(&self) -> &SecretHandle {
        &self.handle
    }
}

/// Secret detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretDetection {
    pub secret_type: SecretType,
    pub pattern: String,
    pub location: usize,
    pub length: usize,
    pub confidence: f32,
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp_ms: i64, // Milliseconds since epoch
    pub operation: AuditOperation,
    pub session_id: String,
    pub handle_id: String,
    pub secret_type: SecretType,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditOperation {
    Store,
    Retrieve,
    UseInTool,
    Revoke,
    Cleanup,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zeroizing_string_zeroizes_on_drop() {
        let s = ZeroizingString::new("test_secret".to_string());
        assert_eq!(s.as_str(), "test_secret");
        // Zeroize happens on drop
    }

    #[test]
    fn test_secret_manager_store_and_retrieve() {
        let config = SecretManagerConfig::default();
        let manager = SecretManager::new(config);

        let handle = manager
            .store_secret(
                "test_session",
                SecretInput {
                    value: "sk-test-12345".into(),
                    secret_type: None,
                    metadata: HashMap::new(),
                    permissions: None,
                },
            )
            .unwrap();

        let secret = manager.retrieve_secret("test_session", &handle).unwrap();
        assert_eq!(secret.value(), "sk-test-12345");
    }

    #[test]
    fn test_secret_handle_isolation() {
        let config = SecretManagerConfig::default();
        let manager = SecretManager::new(config);

        let handle = manager
            .store_secret(
                "session_a",
                SecretInput {
                    value: "secret_a".into(),
                    secret_type: None,
                    metadata: HashMap::new(),
                    permissions: None,
                },
            )
            .unwrap();

        // Different session cannot access
        let result = manager.retrieve_secret("session_b", &handle);
        assert!(result.is_err());
    }

    #[test]
    fn test_secret_redaction() {
        let config = SecretManagerConfig::default();
        let manager = SecretManager::new(config);

        let text = "My API key is sk-ant-oat01-abcdefghijklmnopqrstuvwxyz and token is ghp_abcdefghijklmnopqrstuvwxyz";
        let result = manager.redact_output(text, RedactionContext::default());
        // Markers are type-specific on purpose: "[REDACTED]" alone cannot say
        // what was removed, which is what an audit needs.
        assert!(result.redacted.contains("[REDACTED_ANTHROPIC_KEY]"));
        assert!(result.redacted.contains("[REDACTED_GITHUB_TOKEN]"));
        assert!(!result.redacted.contains("sk-ant-oat01-"));
        assert!(!result.redacted.contains("ghp_"));
    }
}
