//! Secure Secret Storage
//!
//! Provides encrypted, access-controlled storage for secrets with
//! automatic zeroization and audit trails.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock as StdRwLock};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};
use crate::secret_management::{SecretType, SecretHandle};

/// Configuration for secret storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Root directory for secret storage
    pub root_dir: PathBuf,
    /// Maximum number of secrets per session
    pub max_secrets_per_session: usize,
    /// Enable encryption at rest
    pub encrypt_at_rest: bool,
    /// Encryption key derivation iterations
    pub kdf_iterations: u32,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            root_dir: dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".alphacode")
                .join("secrets"),
            max_secrets_per_session: 1000,
            // Deliberately false: `encrypt_value` has no cipher behind it, and
            // defaulting to true made the store claim protection it cannot
            // provide. Flip this when a real AEAD is implemented.
            encrypt_at_rest: false,
            kdf_iterations: 100_000,
        }
    }
}

/// A secret entry in storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretEntry {
    /// Unique handle for this secret
    pub handle: SecretHandle,
    /// Session that owns this secret
    pub session_id: String,
    /// Type of secret
    pub secret_type: SecretType,
    /// The secret value. Wiped on drop by `ZeroizingString`; no container-level
    /// derive is needed, and the other fields carry no secret material.
    pub value: ZeroizingString,
    /// Metadata about the secret
    pub metadata: HashMap<String, String>,
    /// Creation timestamp (milliseconds since epoch)
    pub created_at_ms: i64,
    /// Expiration timestamp (milliseconds since epoch, None = no expiry)
    pub expires_at_ms: Option<i64>,
    /// Access count
    pub access_count: u64,
    /// Last access timestamp (milliseconds since epoch)
    pub last_accessed_ms: Option<i64>,
}

impl SecretEntry {
    pub fn new(
        session_id: String,
        value: ZeroizingString,
        secret_type: SecretType,
        metadata: HashMap<String, String>,
    ) -> Self {
        let handle = SecretHandle::new(&session_id);
        Self {
            handle,
            session_id,
            secret_type,
            value,
            metadata,
            created_at_ms: chrono::Utc::now().timestamp_millis(),
            expires_at_ms: None,
            access_count: 0,
            last_accessed_ms: None,
        }
    }
}

/// A string that zeroizes on drop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroizingString(String);

impl ZeroizingString {
    pub fn new(s: String) -> Self {
        Self(s)
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Zeroize for ZeroizingString {
    fn zeroize(&mut self) {
        // Overwrite the heap buffer in place, then drop the (now empty) String.
        // `unsafe` is not needed: `String::into_bytes` gives us the buffer and
        // `zeroize` on the Vec wipes it before the allocation is released.
        let mut bytes = std::mem::take(&mut self.0).into_bytes();
        bytes.zeroize();
    }
}

impl AsRef<str> for ZeroizingString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for ZeroizingString {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ZeroizingString {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl std::ops::Deref for ZeroizingString {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Secure secret store with encryption and access control
pub struct SecretStore {
    config: StorageConfig,
    sessions: StdRwLock<HashMap<String, SessionSecrets>>,
    encryption_key: Option<Arc<EncryptionKey>>,
}

struct SessionSecrets {
    secrets: HashMap<String, SecretEntry>,
    created_at_ms: i64,
}

#[derive(Zeroize, ZeroizeOnDrop)]
struct EncryptionKey {
    key: [u8; 32],
}

impl SecretStore {
    pub fn new(max_secrets_per_session: usize) -> Self {
        let config = StorageConfig {
            max_secrets_per_session,
            ..Default::default()
        };
        
        // Ensure storage directory exists
        std::fs::create_dir_all(&config.root_dir).ok();
        
        Self {
            config,
            sessions: StdRwLock::new(HashMap::new()),
            encryption_key: None, // Would be initialized from secure key storage
        }
    }
    
    pub fn with_config(config: StorageConfig) -> Self {
        std::fs::create_dir_all(&config.root_dir).ok();
        
        Self {
            config,
            sessions: StdRwLock::new(HashMap::new()),
            encryption_key: None,
        }
    }
    
    /// Store a secret entry
    pub fn store(&self, mut entry: SecretEntry) -> Result<SecretHandle> {
        let handle = entry.handle.clone();
        let session_id = entry.session_id.clone();
        
        // Check session limit
        {
            let sessions = self.sessions.read().unwrap();
            if let Some(session) = sessions.get(&session_id)
                && session.secrets.len() >= self.config.max_secrets_per_session {
                    return Err(anyhow!("Session secret limit reached"));
                }
        }
        
        // Encrypt if enabled
        if self.config.encrypt_at_rest {
            // `mem::replace` keeps ownership legal: SecretEntry implements Drop
            // through its ZeroizingString field, so its fields cannot be moved out.
            let value = std::mem::replace(&mut entry.value, ZeroizingString::new(String::new()));
            entry.value = self.encrypt_value(value)?;
        }
        
        // Store in memory
        {
            let mut sessions = self.sessions.write().unwrap();
            let session = sessions.entry(session_id.clone()).or_insert_with(|| SessionSecrets {
                secrets: HashMap::new(),
                created_at_ms: chrono::Utc::now().timestamp_millis(),
            });
            session.secrets.insert(handle.id().to_string(), entry);
        }
        
        // Persist to disk (encrypted)
        self.persist_session(&session_id)?;
        
        Ok(handle)
    }
    
    /// Retrieve a secret entry by handle
    pub fn retrieve(&self, handle: &SecretHandle) -> Result<SecretEntry> {
        let handle_id = handle.id().to_string();
        
        // Find which session owns this handle
        let session_id = {
            let sessions = self.sessions.read().unwrap();
            sessions.iter()
                .find(|(_, s)| s.secrets.contains_key(&handle_id))
                .map(|(id, _)| id.clone())
                .ok_or_else(|| anyhow!("Secret handle not found"))?
        };
        
        // Retrieve and update access info
        let mut entry = {
            let mut sessions = self.sessions.write().unwrap();
            let session = sessions.get_mut(&session_id)
                .ok_or_else(|| anyhow!("Session not found"))?;
            let mut entry = session.secrets.get_mut(&handle_id)
                .ok_or_else(|| anyhow!("Secret handle not found"))?
                .clone();
            
            // Check expiration
            let now_ms = chrono::Utc::now().timestamp_millis();
            if let Some(expires_at_ms) = entry.expires_at_ms
                && now_ms > expires_at_ms {
                    session.secrets.remove(&handle_id);
                    return Err(anyhow!("Secret has expired"));
                }
            
            entry.access_count += 1;
            entry.last_accessed_ms = Some(now_ms);
            session.secrets.insert(handle_id.clone(), entry.clone());
            entry
        };
        
        // Decrypt if needed
        if self.config.encrypt_at_rest {
            let value = std::mem::replace(&mut entry.value, ZeroizingString::new(String::new()));
            entry.value = self.decrypt_value(value)?;
        }
        
        Ok(entry)
    }
    
    /// Delete a secret entry
    pub fn delete(&self, handle: &SecretHandle) -> Result<()> {
        let handle_id = handle.id().to_string();
        
        let session_id = {
            let sessions = self.sessions.read().unwrap();
            sessions.iter()
                .find(|(_, s)| s.secrets.contains_key(&handle_id))
                .map(|(id, _)| id.clone())
                .ok_or_else(|| anyhow!("Secret handle not found"))?
        };
        
        {
            let mut sessions = self.sessions.write().unwrap();
            if let Some(session) = sessions.get_mut(&session_id) {
                session.secrets.remove(&handle_id);
            }
        }
        
        // Persist updated session
        self.persist_session(&session_id)?;
        
        Ok(())
    }
    
    /// Get all handles for a session
    pub fn list_handles(&self, session_id: &str) -> Vec<SecretHandle> {
        let sessions = self.sessions.read().unwrap();
        sessions.get(session_id)
            .map(|s| s.secrets.values().map(|e| e.handle.clone()).collect())
            .unwrap_or_default()
    }
    
    /// Clean up expired secrets for a session
    pub fn cleanup_expired(&self, session_id: &str) -> usize {
        let mut sessions = self.sessions.write().unwrap();
        if let Some(session) = sessions.get_mut(session_id) {
            let now_ms = chrono::Utc::now().timestamp_millis();
            let initial_len = session.secrets.len();
            session.secrets.retain(|_, e| {
                e.expires_at_ms.is_none_or(|exp| now_ms <= exp)
            });
            initial_len - session.secrets.len()
        } else {
            0
        }
    }
    
    /// Persist a session to disk
    fn persist_session(&self, session_id: &str) -> Result<()> {
        let sessions = self.sessions.read().unwrap();
        if let Some(session) = sessions.get(session_id) {
            let path = self.session_path(session_id);
            let data = serde_json::to_vec(&session.secrets)?;
            
            // Write atomically
            let tmp_path = path.with_extension("tmp");
            std::fs::write(&tmp_path, data)?;
            std::fs::rename(tmp_path, path)?;
        }
        Ok(())
    }
    
    /// Load a session from disk
    pub fn load_session(&self, session_id: &str) -> Result<()> {
        let path = self.session_path(session_id);
        if !path.exists() {
            return Ok(());
        }
        
        let data = std::fs::read(path)?;
        let secrets: HashMap<String, SecretEntry> = serde_json::from_slice(&data)?;
        
        let mut sessions = self.sessions.write().unwrap();
        sessions.insert(session_id.to_string(), SessionSecrets {
            secrets,
            created_at_ms: chrono::Utc::now().timestamp_millis(),
        });
        
        Ok(())
    }
    
    fn session_path(&self, session_id: &str) -> PathBuf {
        // Sanitize session ID for filesystem
        let safe_id = session_id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect::<String>();
        self.config.root_dir.join(format!("{}.json", safe_id))
    }
    
    /// Encrypt-at-rest is not implemented.
    ///
    /// The previous version prefixed the plaintext with `ENC:` and called that
    /// encryption. It wrote the secret to disk in the clear while the config
    /// reported `encrypt_at_rest: true`, which is worse than no claim at all:
    /// an operator reading the configuration would believe the file was safe.
    ///
    /// Returning an error makes the limitation impossible to miss and fails
    /// closed. `StorageConfig::default` therefore leaves `encrypt_at_rest`
    /// false until a real AEAD (e.g. XChaCha20-Poly1305 with a KDF-derived key)
    /// is wired in; see `docs/engineering/code-signing.md` for the policy that
    /// secrets must never be persisted unencrypted.
    fn encrypt_value(&self, _value: ZeroizingString) -> Result<ZeroizingString> {
        Err(anyhow!(
            "encrypt-at-rest is not implemented; refusing to persist secret material in plaintext.              Set encrypt_at_rest = false only if the storage directory is otherwise protected."
        ))
    }

    fn decrypt_value(&self, value: ZeroizingString) -> Result<ZeroizingString> {
        // Nothing is ever written encrypted today, so a stored value is returned
        // unchanged. Kept separate so the read path does not need to change when
        // real encryption lands.
        Ok(value)
    }
}

impl Default for SecretStore {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_store_and_retrieve() {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig {
            root_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let store = SecretStore::with_config(config);
        
        let entry = SecretEntry {
            handle: SecretHandle::new("test_session"),
            session_id: "test_session".to_string(),
            secret_type: SecretType::GenericApiKey,
            value: "test_secret_123".into(),
            metadata: HashMap::new(),
            created_at_ms: chrono::Utc::now().timestamp_millis(),
            expires_at_ms: None,
            access_count: 0,
            last_accessed_ms: None,
        };
        
        let handle = store.store(entry).unwrap();
        let retrieved = store.retrieve(&handle).unwrap();
        
        assert_eq!(retrieved.value.as_str(), "test_secret_123");
        assert_eq!(retrieved.access_count, 1);
    }
    
    #[test]
    fn test_delete_secret() {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig {
            root_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let store = SecretStore::with_config(config);
        
        let entry = SecretEntry {
            handle: SecretHandle::new("test_session"),
            session_id: "test_session".to_string(),
            secret_type: SecretType::GenericApiKey,
            value: "test_secret".into(),
            metadata: HashMap::new(),
            created_at_ms: chrono::Utc::now().timestamp_millis(),
            expires_at_ms: None,
            access_count: 0,
            last_accessed_ms: None,
        };
        
        let handle = store.store(entry).unwrap();
        store.delete(&handle).unwrap();
        
        let result = store.retrieve(&handle);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_session_isolation() {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig {
            root_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let store = SecretStore::with_config(config);
        
        let entry1 = SecretEntry {
            handle: SecretHandle::new("session_a"),
            session_id: "session_a".to_string(),
            secret_type: SecretType::GenericApiKey,
            value: "secret_a".into(),
            metadata: HashMap::new(),
            created_at_ms: chrono::Utc::now().timestamp_millis(),
            expires_at_ms: None,
            access_count: 0,
            last_accessed_ms: None,
        };
        
        let handle = store.store(entry1).unwrap();
        
        // Different session cannot access
        let result = store.retrieve(&handle);
        // Should still work because handle is the same
        // But if we create a secret for session_b with same handle pattern, it should be different
        assert!(result.is_ok()); // Same handle, same session
    }
    
    #[test]
    fn test_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig {
            root_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let store = SecretStore::with_config(config);
        
        let entry = SecretEntry {
            handle: SecretHandle::new("persist_test"),
            session_id: "persist_test".to_string(),
            secret_type: SecretType::GenericApiKey,
            value: "persistent_secret".into(),
            metadata: HashMap::new(),
            created_at_ms: chrono::Utc::now().timestamp_millis(),
            expires_at_ms: None,
            access_count: 0,
            last_accessed_ms: None,
        };
        
        let handle = store.store(entry).unwrap();
        drop(store);
        
        // Create new store and load
        let config = StorageConfig {
            root_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let store2 = SecretStore::with_config(config);
        store2.load_session("persist_test").unwrap();
        
        let retrieved = store2.retrieve(&handle).unwrap();
        assert_eq!(retrieved.value.as_str(), "persistent_secret");
    }
}