//! Secret Handle System
//!
//! Provides opaque handles for secrets that allow authorized tools to use
//! credentials without revealing plaintext values to the model.

use anyhow::{Result, anyhow};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::RwLock as StdRwLock;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Standard base64 (padded), matching the encoding this module has always used.
const BASE64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::STANDARD;

/// Opaque handle for a secret
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SecretHandle {
    id: String,
    session_id: String,
    created_at_ms: i64, // Milliseconds since epoch
}

impl SecretHandle {
    pub fn new(session_id: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            created_at_ms: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn with_id(id: String, session_id: &str) -> Self {
        Self {
            id,
            session_id: session_id.to_string(),
            created_at_ms: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }

    pub fn created_at(&self) -> Instant {
        // For backward compatibility, convert back to Instant
        // This is approximate since Instant is not epoch-based
        Instant::now()
            - Duration::from_millis(
                (chrono::Utc::now().timestamp_millis() - self.created_at_ms).max(0) as u64,
            )
    }
}

impl fmt::Display for SecretHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Handles are printed in logs and error messages. `with_id` accepts a
        // caller-supplied id that may be shorter than 8 bytes, so the prefix is
        // taken by character, not by byte slice: `&self.id[..8]` panics on a
        // short or multi-byte id, and a panic in Display would take down
        // logging while handling a secret.
        let prefix: String = self.id.chars().take(8).collect();
        write!(f, "secret:{prefix}")
    }
}

/// Permissions for a secret handle.
///
/// The derived `Default` was all-false, which made every secret stored without
/// explicit permissions unreadable: `store_secret` accepted it, then
/// `retrieve_secret` failed with "Handle does not have read permission". A
/// handle is only useful if its owner can read it, so the default is `full()`.
/// Callers that want a write-only secret pass `tool_only()` explicitly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandlePermissions {
    /// Allow reading the secret value (for tool execution)
    pub can_read: bool,
    /// Allow using in tool calls (without exposing to model)
    pub can_use_in_tool: bool,
    /// Allow listing in handle enumeration
    pub can_list: bool,
    /// Allow revoking
    pub can_revoke: bool,
    /// Maximum number of uses (None = unlimited)
    pub max_uses: Option<u64>,
}

impl Default for HandlePermissions {
    fn default() -> Self {
        Self::full()
    }
}

impl HandlePermissions {
    pub fn full() -> Self {
        Self {
            can_read: true,
            can_use_in_tool: true,
            can_list: true,
            can_revoke: true,
            max_uses: None,
        }
    }

    pub fn tool_only() -> Self {
        Self {
            can_read: false,
            can_use_in_tool: true,
            can_list: true,
            can_revoke: false,
            max_uses: None,
        }
    }

    pub fn read_only() -> Self {
        Self {
            can_read: true,
            can_use_in_tool: false,
            can_list: true,
            can_revoke: false,
            max_uses: None,
        }
    }
}

/// Metadata associated with a handle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandleMetadata {
    pub session_id: String,
    pub secret_type: crate::secret_management::SecretType,
    pub created_at_ms: i64,         // Milliseconds since epoch
    pub expires_at_ms: Option<i64>, // Milliseconds since epoch
    pub permissions: HandlePermissions,
    pub access_count: u64,
    pub last_accessed_ms: Option<i64>, // Milliseconds since epoch
    pub tags: Vec<String>,
}

/// Registry for secret handles with access control
pub struct SecretHandleRegistry {
    handles: StdRwLock<HashMap<String, HandleMetadata>>,
    max_handles: usize,
}

impl SecretHandleRegistry {
    pub fn new(max_handles: usize) -> Self {
        Self {
            handles: StdRwLock::new(HashMap::new()),
            max_handles,
        }
    }

    /// Register a new handle
    pub fn register(&self, handle: SecretHandle, metadata: HandleMetadata) -> Result<()> {
        let mut handles = self.handles.write().unwrap();

        if handles.len() >= self.max_handles {
            return Err(anyhow!("Handle registry full"));
        }

        if handles.contains_key(handle.id()) {
            return Err(anyhow!("Handle already exists"));
        }

        handles.insert(handle.id().to_string(), metadata);
        Ok(())
    }

    /// Get handle metadata
    pub fn get(&self, handle: &SecretHandle) -> Result<HandleMetadata> {
        let handles = self.handles.read().unwrap();
        handles
            .get(handle.id())
            .cloned()
            .ok_or_else(|| anyhow!("Handle not found"))
    }

    /// Increment access count
    pub fn increment_access(&self, handle: &SecretHandle) -> Result<()> {
        let mut handles = self.handles.write().unwrap();
        if let Some(metadata) = handles.get_mut(handle.id()) {
            metadata.access_count += 1;
            metadata.last_accessed_ms = Some(chrono::Utc::now().timestamp_millis());

            // Check max uses
            if let Some(max) = metadata.permissions.max_uses
                && metadata.access_count > max
            {
                return Err(anyhow!("Handle usage limit exceeded"));
            }
            Ok(())
        } else {
            Err(anyhow!("Handle not found"))
        }
    }

    /// Revoke a handle
    pub fn revoke(&self, handle: &SecretHandle) -> Result<()> {
        let mut handles = self.handles.write().unwrap();
        handles.remove(handle.id());
        Ok(())
    }

    /// List all handles for a session
    pub fn list_session_handles(&self, session_id: &str) -> Vec<(SecretHandle, HandleMetadata)> {
        let handles = self.handles.read().unwrap();
        handles
            .iter()
            .filter(|(_, m)| m.session_id == session_id)
            .filter(|(_, m)| m.permissions.can_list)
            .map(|(id, m)| {
                let handle = SecretHandle::with_id(id.clone(), &m.session_id);
                (handle, m.clone())
            })
            .collect()
    }

    /// Get expired handles
    pub fn get_expired(&self) -> Vec<SecretHandle> {
        let handles = self.handles.read().unwrap();
        let now_ms = chrono::Utc::now().timestamp_millis();
        handles
            .iter()
            .filter(|(_, m)| m.expires_at_ms.is_some_and(|exp| now_ms > exp))
            .map(|(id, m)| SecretHandle::with_id(id.clone(), &m.session_id))
            .collect()
    }

    /// Get handle count for a session
    pub fn session_handle_count(&self, session_id: &str) -> usize {
        let handles = self.handles.read().unwrap();
        handles
            .values()
            .filter(|m| m.session_id == session_id)
            .count()
    }

    /// Clean up expired handles
    pub fn cleanup_expired(&self) -> usize {
        let mut handles = self.handles.write().unwrap();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let initial = handles.len();
        handles.retain(|_, m| m.expires_at_ms.is_none_or(|exp| now_ms <= exp));
        initial - handles.len()
    }
}

impl Default for SecretHandleRegistry {
    fn default() -> Self {
        Self::new(1000)
    }
}

/// A secret reference that can be used in tool arguments
/// This replaces plaintext secrets in tool calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretReference {
    pub handle: SecretHandle,
    /// Optional: specific field to extract (for structured secrets)
    pub field: Option<String>,
    /// Optional: transformation to apply
    pub transform: Option<SecretTransform>,
}

/// Transformations that can be applied to secrets
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum SecretTransform {
    /// Use as-is
    #[default]
    None,
    /// Base64 encode
    Base64Encode,
    /// Base64 decode
    Base64Decode,
    /// URL encode
    UrlEncode,
    /// Extract a JSON field
    JsonExtract(String),
    /// Use as Bearer token
    BearerToken,
    /// Use as Basic auth
    BasicAuth(String), // username
}

/// Build a secret reference for use in tool arguments
pub fn secret_ref(handle: SecretHandle) -> SecretReference {
    SecretReference {
        handle,
        field: None,
        transform: None,
    }
}

/// Build a secret reference with transformation
pub fn secret_ref_transformed(handle: SecretHandle, transform: SecretTransform) -> SecretReference {
    SecretReference {
        handle,
        field: None,
        transform: Some(transform),
    }
}

/// Resolve a secret reference to its value (for authorized tool use only)
pub fn resolve_secret_ref(
    manager: &crate::secret_management::SecretManager,
    session_id: &str,
    reference: &SecretReference,
) -> Result<String> {
    let secret = manager.retrieve_secret(session_id, &reference.handle)?;
    let mut value = secret.value().to_string();

    // Apply field extraction if specified
    if let Some(field) = &reference.field
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&value)
        && let Some(extracted) = json.get(field)
    {
        value = extracted.as_str().unwrap_or("").to_string();
    }

    // Apply transformation
    if let Some(transform) = &reference.transform {
        value = match transform {
            SecretTransform::None => value,
            SecretTransform::Base64Encode => BASE64.encode(value),
            SecretTransform::Base64Decode => String::from_utf8(BASE64.decode(value)?)?,
            SecretTransform::UrlEncode => urlencoding::encode(&value).to_string(),
            SecretTransform::JsonExtract(path) => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&value) {
                    json.pointer(path)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                } else {
                    value
                }
            }
            SecretTransform::BearerToken => format!("Bearer {}", value),
            SecretTransform::BasicAuth(username) => {
                let credentials = format!("{}:{}", username, value);
                BASE64.encode(credentials)
            }
        };
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_creation() {
        let handle = SecretHandle::new("test_session");
        assert!(!handle.id().is_empty());
        assert_eq!(handle.session_id(), "test_session");
    }

    #[test]
    fn test_handle_registry() {
        let registry = SecretHandleRegistry::new(10);
        let handle = SecretHandle::new("test_session");

        let metadata = HandleMetadata {
            session_id: "test_session".to_string(),
            secret_type: crate::secret_management::SecretType::GenericApiKey,
            created_at_ms: chrono::Utc::now().timestamp_millis(),
            expires_at_ms: None,
            permissions: HandlePermissions::full(),
            access_count: 0,
            last_accessed_ms: None,
            tags: vec![],
        };

        registry.register(handle.clone(), metadata).unwrap();
        let retrieved = registry.get(&handle).unwrap();
        assert_eq!(retrieved.session_id, "test_session");
    }

    #[test]
    fn test_handle_expiry() {
        let registry = SecretHandleRegistry::new(10);
        let handle = SecretHandle::new("test_session");

        let metadata = HandleMetadata {
            session_id: "test_session".to_string(),
            secret_type: crate::secret_management::SecretType::GenericApiKey,
            created_at_ms: chrono::Utc::now().timestamp_millis(),
            expires_at_ms: Some(chrono::Utc::now().timestamp_millis() + 10),
            permissions: HandlePermissions::full(),
            access_count: 0,
            last_accessed_ms: None,
            tags: vec![],
        };

        registry.register(handle.clone(), metadata).unwrap();

        std::thread::sleep(Duration::from_millis(20));

        let expired = registry.get_expired();
        assert_eq!(expired.len(), 1);
    }

    #[test]
    fn test_secret_reference_resolution() {
        let config = crate::secret_management::SecretManagerConfig::default();
        let manager = crate::secret_management::SecretManager::new(config);

        let handle = manager
            .store_secret(
                "test_session",
                crate::secret_management::SecretInput {
                    value: "test_api_key".into(),
                    secret_type: None,
                    metadata: Default::default(),
                    permissions: None,
                },
            )
            .unwrap();

        let reference = secret_ref(handle);
        let resolved = resolve_secret_ref(&manager, "test_session", &reference).unwrap();
        assert_eq!(resolved, "test_api_key");
    }

    #[test]
    fn test_secret_transform_bearer() {
        let config = crate::secret_management::SecretManagerConfig::default();
        let manager = crate::secret_management::SecretManager::new(config);

        let handle = manager
            .store_secret(
                "test_session",
                crate::secret_management::SecretInput {
                    value: "my_token".into(),
                    secret_type: None,
                    metadata: Default::default(),
                    permissions: None,
                },
            )
            .unwrap();

        let reference = secret_ref_transformed(handle, SecretTransform::BearerToken);
        let resolved = resolve_secret_ref(&manager, "test_session", &reference).unwrap();
        assert_eq!(resolved, "Bearer my_token");
    }
}
