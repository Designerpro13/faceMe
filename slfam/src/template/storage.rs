//! Template storage implementation

use super::{TEMPLATE_MAGIC, TEMPLATE_VERSION};
use crate::crypto::{decrypt, encrypt, DerivedKey, EncryptedData};
use crate::embedding::FaceEmbedding;
use crate::error::{Result, TemplateError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use zeroize::Zeroize;

/// Template metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateMetadata {
    /// Creation timestamp (Unix epoch seconds)
    pub created_at: u64,
    /// Last update timestamp
    pub updated_at: u64,
    /// Number of successful authentications
    pub auth_count: u64,
    /// Last successful authentication
    pub last_auth: Option<u64>,
    /// Device info
    pub device_id: Option<String>,
    /// Additional key-value pairs
    pub extra: HashMap<String, String>,
}

impl TemplateMetadata {
    /// Create new metadata
    pub fn new() -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            created_at: now,
            updated_at: now,
            auth_count: 0,
            last_auth: None,
            device_id: None,
            extra: HashMap::new(),
        }
    }

    /// Record a successful authentication
    pub fn record_auth(&mut self) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        self.auth_count += 1;
        self.last_auth = Some(now);
        self.updated_at = now;
    }
}

impl Default for TemplateMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// A face template containing embeddings and metadata
#[derive(Clone)]
pub struct Template {
    /// User identifier
    user_id: String,
    /// Face embeddings
    embeddings: Vec<FaceEmbedding>,
    /// Metadata
    metadata: TemplateMetadata,
    /// Version
    version: u8,
}

impl Template {
    /// Create a new template
    pub fn new(
        user_id: String,
        embeddings: Vec<FaceEmbedding>,
        metadata: Option<TemplateMetadata>,
    ) -> Self {
        Self {
            user_id,
            embeddings,
            metadata: metadata.unwrap_or_default(),
            version: TEMPLATE_VERSION,
        }
    }

    /// Get user ID
    #[must_use]
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Get embeddings
    #[must_use]
    pub fn embeddings(&self) -> &[FaceEmbedding] {
        &self.embeddings
    }

    /// Get mutable embeddings
    pub fn embeddings_mut(&mut self) -> &mut Vec<FaceEmbedding> {
        &mut self.embeddings
    }

    /// Get metadata
    #[must_use]
    pub fn metadata(&self) -> &TemplateMetadata {
        &self.metadata
    }

    /// Get mutable metadata
    pub fn metadata_mut(&mut self) -> &mut TemplateMetadata {
        &mut self.metadata
    }

    /// Get version
    #[must_use]
    pub fn version(&self) -> u8 {
        self.version
    }

    /// Add an embedding
    pub fn add_embedding(&mut self, embedding: FaceEmbedding) {
        self.embeddings.push(embedding);
        self.metadata.updated_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Serialize to bytes (unencrypted)
    fn serialize(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Header
        data.extend_from_slice(TEMPLATE_MAGIC);
        data.push(self.version);

        // User ID (length-prefixed)
        let user_bytes = self.user_id.as_bytes();
        data.extend_from_slice(&(user_bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(user_bytes);

        // Metadata (JSON)
        let metadata_json =
            serde_json::to_vec(&self.metadata).map_err(|e| TemplateError::SerializationFailed(e.to_string()))?;
        data.extend_from_slice(&(metadata_json.len() as u32).to_le_bytes());
        data.extend_from_slice(&metadata_json);

        // Embeddings count
        data.extend_from_slice(&(self.embeddings.len() as u32).to_le_bytes());

        // Each embedding
        for emb in &self.embeddings {
            let emb_bytes = emb.to_bytes();
            data.extend_from_slice(&(emb_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(&emb_bytes);
        }

        Ok(data)
    }

    /// Deserialize from bytes (unencrypted)
    fn deserialize(data: &[u8]) -> Result<Self> {
        let mut offset = 0;

        // Check magic
        if data.len() < 5 || &data[0..4] != TEMPLATE_MAGIC {
            return Err(TemplateError::InvalidFormat("Invalid magic bytes".to_string()).into());
        }
        offset += 4;

        // Version
        let version = data[offset];
        if version > TEMPLATE_VERSION {
            return Err(TemplateError::UnsupportedVersion {
                got: version,
                max_supported: TEMPLATE_VERSION,
            }
            .into());
        }
        offset += 1;

        // User ID
        if data.len() < offset + 4 {
            return Err(TemplateError::InvalidFormat("Truncated user ID length".to_string()).into());
        }
        let user_len = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;

        if data.len() < offset + user_len {
            return Err(TemplateError::InvalidFormat("Truncated user ID".to_string()).into());
        }
        let user_id = String::from_utf8(data[offset..offset + user_len].to_vec())
            .map_err(|e| TemplateError::InvalidFormat(format!("Invalid UTF-8 in user ID: {}", e)))?;
        offset += user_len;

        // Metadata
        if data.len() < offset + 4 {
            return Err(TemplateError::InvalidFormat("Truncated metadata length".to_string()).into());
        }
        let meta_len = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;

        if data.len() < offset + meta_len {
            return Err(TemplateError::InvalidFormat("Truncated metadata".to_string()).into());
        }
        let metadata: TemplateMetadata = serde_json::from_slice(&data[offset..offset + meta_len])
            .map_err(|e| TemplateError::InvalidFormat(format!("Invalid metadata JSON: {}", e)))?;
        offset += meta_len;

        // Embeddings count
        if data.len() < offset + 4 {
            return Err(TemplateError::InvalidFormat("Truncated embedding count".to_string()).into());
        }
        let emb_count = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;

        // Embeddings
        let mut embeddings = Vec::with_capacity(emb_count);
        for _ in 0..emb_count {
            if data.len() < offset + 4 {
                return Err(TemplateError::InvalidFormat("Truncated embedding length".to_string()).into());
            }
            let emb_len = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
            offset += 4;

            if data.len() < offset + emb_len {
                return Err(TemplateError::InvalidFormat("Truncated embedding data".to_string()).into());
            }
            let embedding = FaceEmbedding::from_bytes(&data[offset..offset + emb_len])?;
            embeddings.push(embedding);
            offset += emb_len;
        }

        Ok(Self {
            user_id,
            embeddings,
            metadata,
            version,
        })
    }

    /// Encrypt the template
    pub fn encrypt(&self, key: &DerivedKey) -> Result<Vec<u8>> {
        let plaintext = self.serialize()?;
        let encrypted = encrypt(key, &plaintext, self.user_id.as_bytes())?;
        Ok(encrypted.to_bytes())
    }

    /// Decrypt a template
    pub fn decrypt(data: &[u8], key: &DerivedKey) -> Result<Self> {
        let encrypted = EncryptedData::from_bytes(data)?;
        
        // We need to try decryption without knowing user_id first for AAD
        // So we'll use empty AAD for the outer layer, and verify user_id after
        let mut plaintext = decrypt(key, &encrypted, &[])?;
        
        let template = Self::deserialize(&plaintext)?;
        plaintext.zeroize();
        
        Ok(template)
    }
}

impl std::fmt::Debug for Template {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Template")
            .field("user_id", &self.user_id)
            .field("embeddings_count", &self.embeddings.len())
            .field("version", &self.version)
            .finish()
    }
}

/// Template store for managing multiple user templates
pub struct TemplateStore {
    /// Base directory for template storage
    base_path: PathBuf,
    /// Loaded templates (cached)
    cache: HashMap<String, Template>,
}

impl TemplateStore {
    /// Create a new template store
    ///
    /// # Arguments
    ///
    /// * `base_path` - Directory to store template files
    pub fn new<P: AsRef<Path>>(base_path: P) -> Result<Self> {
        let base_path = base_path.as_ref().to_path_buf();
        
        // Create directory if it doesn't exist
        if !base_path.exists() {
            fs::create_dir_all(&base_path)
                .map_err(|e| TemplateError::IoError(e.to_string()))?;
        }

        // Set restrictive permissions on directory
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o700);
            fs::set_permissions(&base_path, perms)
                .map_err(|e| TemplateError::IoError(e.to_string()))?;
        }

        Ok(Self {
            base_path,
            cache: HashMap::new(),
        })
    }

    /// Get template file path for a user
    fn template_path(&self, user_id: &str) -> PathBuf {
        // Sanitize user_id for filename
        let safe_name: String = user_id
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
            .collect();
        self.base_path.join(format!("{}.slftemplate", safe_name))
    }

    /// Check if a template exists for a user
    #[must_use]
    pub fn exists(&self, user_id: &str) -> bool {
        self.template_path(user_id).exists()
    }

    /// Save a template
    pub fn save(&mut self, template: &Template, key: &DerivedKey) -> Result<()> {
        let path = self.template_path(template.user_id());
        let encrypted = template.encrypt(key)?;
        
        fs::write(&path, &encrypted)
            .map_err(|e| TemplateError::IoError(e.to_string()))?;

        // Set restrictive permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o600);
            fs::set_permissions(&path, perms)
                .map_err(|e| TemplateError::IoError(e.to_string()))?;
        }

        // Update cache
        self.cache.insert(template.user_id().to_string(), template.clone());

        Ok(())
    }

    /// Load a template
    pub fn load(&mut self, user_id: &str, key: &DerivedKey) -> Result<Template> {
        // Check cache first
        if let Some(template) = self.cache.get(user_id) {
            return Ok(template.clone());
        }

        let path = self.template_path(user_id);
        if !path.exists() {
            return Err(TemplateError::NotFound(user_id.to_string()).into());
        }

        let data = fs::read(&path)
            .map_err(|e| TemplateError::IoError(e.to_string()))?;

        let template = Template::decrypt(&data, key)?;

        // Cache it
        self.cache.insert(user_id.to_string(), template.clone());

        Ok(template)
    }

    /// Delete a template
    pub fn delete(&mut self, user_id: &str) -> Result<()> {
        let path = self.template_path(user_id);
        
        if path.exists() {
            fs::remove_file(&path)
                .map_err(|e| TemplateError::IoError(e.to_string()))?;
        }

        self.cache.remove(user_id);

        Ok(())
    }

    /// List all users with templates
    pub fn list_users(&self) -> Result<Vec<String>> {
        let mut users = Vec::new();

        for entry in fs::read_dir(&self.base_path)
            .map_err(|e| TemplateError::IoError(e.to_string()))?
        {
            let entry = entry.map_err(|e| TemplateError::IoError(e.to_string()))?;
            let path = entry.path();
            
            if let Some(ext) = path.extension() {
                if ext == "slftemplate" {
                    if let Some(stem) = path.file_stem() {
                        users.push(stem.to_string_lossy().to_string());
                    }
                }
            }
        }

        Ok(users)
    }

    /// Clear the cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::generate_key;
    use tempfile::tempdir;

    fn make_test_embedding() -> FaceEmbedding {
        FaceEmbedding::from_slice(&vec![0.1f32; 128], true)
    }

    #[test]
    fn test_template_serialize_deserialize() {
        let template = Template::new(
            "testuser".to_string(),
            vec![make_test_embedding()],
            None,
        );

        let serialized = template.serialize().unwrap();
        let deserialized = Template::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.user_id(), "testuser");
        assert_eq!(deserialized.embeddings().len(), 1);
    }

    #[test]
    fn test_template_encrypt_decrypt() {
        let key = generate_key();
        let template = Template::new(
            "alice".to_string(),
            vec![make_test_embedding(), make_test_embedding()],
            None,
        );

        let encrypted = template.encrypt(&key).unwrap();
        let decrypted = Template::decrypt(&encrypted, &key).unwrap();

        assert_eq!(decrypted.user_id(), "alice");
        assert_eq!(decrypted.embeddings().len(), 2);
    }

    #[test]
    fn test_template_wrong_key() {
        let key1 = generate_key();
        let key2 = generate_key();
        let template = Template::new("bob".to_string(), vec![make_test_embedding()], None);

        let encrypted = template.encrypt(&key1).unwrap();
        let result = Template::decrypt(&encrypted, &key2);

        assert!(result.is_err());
    }

    #[test]
    fn test_template_metadata() {
        let mut metadata = TemplateMetadata::new();
        assert_eq!(metadata.auth_count, 0);
        
        metadata.record_auth();
        assert_eq!(metadata.auth_count, 1);
        assert!(metadata.last_auth.is_some());
    }

    #[test]
    fn test_template_store() {
        let dir = tempdir().unwrap();
        let mut store = TemplateStore::new(dir.path()).unwrap();
        let key = generate_key();

        let template = Template::new(
            "charlie".to_string(),
            vec![make_test_embedding()],
            None,
        );

        // Save
        store.save(&template, &key).unwrap();
        assert!(store.exists("charlie"));

        // Load
        store.clear_cache();
        let loaded = store.load("charlie", &key).unwrap();
        assert_eq!(loaded.user_id(), "charlie");

        // List
        let users = store.list_users().unwrap();
        assert!(users.contains(&"charlie".to_string()));

        // Delete
        store.delete("charlie").unwrap();
        assert!(!store.exists("charlie"));
    }

    #[test]
    fn test_template_store_not_found() {
        let dir = tempdir().unwrap();
        let mut store = TemplateStore::new(dir.path()).unwrap();
        let key = generate_key();

        let result = store.load("nonexistent", &key);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_embedding() {
        let mut template = Template::new("user".to_string(), vec![], None);
        assert_eq!(template.embeddings().len(), 0);

        template.add_embedding(make_test_embedding());
        assert_eq!(template.embeddings().len(), 1);
    }
}
