//! # Template Storage Module
//!
//! Manages encrypted face templates with versioning and metadata.

mod storage;

pub use storage::{Template, TemplateMetadata, TemplateStore};

use crate::crypto::DerivedKey;
use crate::embedding::FaceEmbedding;
use crate::error::Result;

/// Current template format version
pub const TEMPLATE_VERSION: u8 = 1;

/// Template file magic bytes
pub const TEMPLATE_MAGIC: &[u8; 4] = b"SLFT";

/// Create a new encrypted template
///
/// # Arguments
///
/// * `user_id` - User identifier
/// * `embeddings` - Face embeddings to store
/// * `key` - Encryption key
/// * `metadata` - Optional metadata
///
/// # Returns
///
/// Encrypted template data
pub fn create_template(
    user_id: &str,
    embeddings: &[FaceEmbedding],
    key: &DerivedKey,
    metadata: Option<TemplateMetadata>,
) -> Result<Vec<u8>> {
    let template = Template::new(user_id.to_string(), embeddings.to_vec(), metadata);
    template.encrypt(key)
}

/// Load and decrypt a template
///
/// # Arguments
///
/// * `data` - Encrypted template data
/// * `key` - Decryption key
///
/// # Returns
///
/// Decrypted template
pub fn load_template(data: &[u8], key: &DerivedKey) -> Result<Template> {
    Template::decrypt(data, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::generate_key;

    #[test]
    fn test_create_load_template() {
        let key = generate_key();
        let embedding = FaceEmbedding::from_slice(&[0.5f32; 128], true);
        
        let encrypted = create_template("testuser", &[embedding.clone()], &key, None).unwrap();
        let loaded = load_template(&encrypted, &key).unwrap();
        
        assert_eq!(loaded.user_id(), "testuser");
        assert_eq!(loaded.embeddings().len(), 1);
    }
}
