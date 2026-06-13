//! Key derivation and management

use super::{KEY_SIZE, random_bytes};
use crate::error::{CryptoError, Result};
use argon2::{Argon2, Algorithm, Params, Version};
use std::fs;
use std::path::Path;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A derived encryption key with secure cleanup
#[derive(Clone, ZeroizeOnDrop)]
pub struct DerivedKey {
    /// The raw key bytes
    #[zeroize]
    key: [u8; KEY_SIZE],
}

impl DerivedKey {
    /// Create a new derived key from bytes
    pub fn new(key: [u8; KEY_SIZE]) -> Self {
        Self { key }
    }

    /// Get the key bytes
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; KEY_SIZE] {
        &self.key
    }

    /// Create from a slice (must be exactly KEY_SIZE bytes)
    pub fn from_slice(slice: &[u8]) -> Result<Self> {
        if slice.len() != KEY_SIZE {
            return Err(CryptoError::InvalidKeyLength {
                expected: KEY_SIZE,
                got: slice.len(),
            }
            .into());
        }

        let mut key = [0u8; KEY_SIZE];
        key.copy_from_slice(slice);
        Ok(Self { key })
    }
}

impl std::fmt::Debug for DerivedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DerivedKey")
            .field("key", &"[REDACTED]")
            .finish()
    }
}

/// Key derivation trait
pub trait KeyDerivation {
    /// Derive a key for a specific user
    fn derive_key(&self, user_id: &str, context: &[u8]) -> Result<DerivedKey>;
}

/// Argon2id-based key derivation from password/passphrase
pub struct PasswordKeyDerivation {
    /// Salt for derivation (should be unique per installation)
    salt: [u8; 16],
    /// Argon2 parameters
    params: Params,
}

impl PasswordKeyDerivation {
    /// Create with a specific salt
    pub fn new(salt: [u8; 16]) -> Self {
        // Conservative Argon2id parameters for security-sensitive use
        let params = Params::new(
            64 * 1024,  // 64 MiB memory
            3,          // 3 iterations
            4,          // 4 parallel lanes
            Some(KEY_SIZE),
        )
        .expect("Valid Argon2 params");

        Self { salt, params }
    }

    /// Create with a random salt
    pub fn with_random_salt() -> Self {
        let mut salt = [0u8; 16];
        random_bytes(&mut salt);
        Self::new(salt)
    }

    /// Get the salt (for storage)
    #[must_use]
    pub fn salt(&self) -> &[u8; 16] {
        &self.salt
    }

    /// Derive key from password
    pub fn derive_from_password(&self, password: &[u8]) -> Result<DerivedKey> {
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, self.params.clone());

        let mut key = [0u8; KEY_SIZE];
        argon2
            .hash_password_into(password, &self.salt, &mut key)
            .map_err(|e| CryptoError::KeyDerivationFailed(e.to_string()))?;

        Ok(DerivedKey::new(key))
    }
}

impl KeyDerivation for PasswordKeyDerivation {
    fn derive_key(&self, user_id: &str, context: &[u8]) -> Result<DerivedKey> {
        // Combine user_id and context as "password"
        let mut input = user_id.as_bytes().to_vec();
        input.extend_from_slice(context);
        
        self.derive_from_password(&input)
    }
}

/// TPM-based key derivation
///
/// When TPM is available, derives device-bound keys using TPM sealing.
/// Falls back to file-based key storage when TPM is unavailable.
pub struct TpmKeyDerivation {
    /// Device-specific master key
    master_key: DerivedKey,
    /// Whether TPM is being used
    using_tpm: bool,
    /// Path to fallback key file
    key_file_path: Option<std::path::PathBuf>,
}

impl TpmKeyDerivation {
    /// Create a new TPM key derivation instance
    ///
    /// # Arguments
    ///
    /// * `key_path` - Path to store/retrieve the master key
    /// * `use_tpm` - Whether to attempt TPM usage
    ///
    /// # Errors
    ///
    /// Returns an error if key cannot be created/loaded
    pub fn new<P: AsRef<Path>>(key_path: P, use_tpm: bool) -> Result<Self> {
        let key_path = key_path.as_ref();

        // Try TPM first if requested
        #[cfg(feature = "tpm")]
        if use_tpm {
            if let Ok(key) = Self::derive_from_tpm() {
                return Ok(Self {
                    master_key: key,
                    using_tpm: true,
                    key_file_path: None,
                });
            }
        }

        // Fall back to file-based key
        let master_key = if key_path.exists() {
            Self::load_key_from_file(key_path)?
        } else {
            let key = super::generate_key();
            Self::save_key_to_file(key_path, &key)?;
            key
        };

        Ok(Self {
            master_key,
            using_tpm: false,
            key_file_path: Some(key_path.to_path_buf()),
        })
    }

    /// Check if TPM is being used
    #[must_use]
    pub fn using_tpm(&self) -> bool {
        self.using_tpm
    }

    /// Load master key from file
    fn load_key_from_file(path: &Path) -> Result<DerivedKey> {
        let data = fs::read(path).map_err(|e| {
            CryptoError::KeyDerivationFailed(format!("Failed to read key file: {}", e))
        })?;

        DerivedKey::from_slice(&data)
    }

    /// Save master key to file with restricted permissions
    fn save_key_to_file(path: &Path, key: &DerivedKey) -> Result<()> {
        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                CryptoError::KeyDerivationFailed(format!("Failed to create key directory: {}", e))
            })?;
        }

        // Write key
        fs::write(path, key.as_bytes()).map_err(|e| {
            CryptoError::KeyDerivationFailed(format!("Failed to write key file: {}", e))
        })?;

        // Set restrictive permissions (owner read only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o400);
            fs::set_permissions(path, perms).map_err(|e| {
                CryptoError::KeyDerivationFailed(format!("Failed to set key permissions: {}", e))
            })?;
        }

        Ok(())
    }

    /// Derive key from TPM (when available)
    #[cfg(feature = "tpm")]
    fn derive_from_tpm() -> Result<DerivedKey> {
        // TPM implementation would go here
        // This would use tpm2-tss crate to:
        // 1. Create or load a primary key
        // 2. Derive a key sealed to the current PCR state
        // 3. Return the derived key
        
        Err(CryptoError::TpmUnavailable.into())
    }

    /// Derive a user-specific key from master key
    fn derive_user_key(&self, user_id: &str, context: &[u8]) -> Result<DerivedKey> {
        // Use HKDF-like expansion
        let mut input = Vec::new();
        input.extend_from_slice(self.master_key.as_bytes());
        input.extend_from_slice(user_id.as_bytes());
        input.extend_from_slice(context);

        // Use Argon2 as a KDF for the final derivation
        let params = Params::new(16 * 1024, 1, 1, Some(KEY_SIZE)).expect("Valid params");
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        // Use user_id as salt for this derivation
        let mut salt = [0u8; 16];
        let user_bytes = user_id.as_bytes();
        for (i, b) in salt.iter_mut().enumerate() {
            *b = user_bytes.get(i).copied().unwrap_or(0);
        }

        let mut key = [0u8; KEY_SIZE];
        argon2
            .hash_password_into(&input, &salt, &mut key)
            .map_err(|e| CryptoError::KeyDerivationFailed(e.to_string()))?;

        input.zeroize();

        Ok(DerivedKey::new(key))
    }
}

impl KeyDerivation for TpmKeyDerivation {
    fn derive_key(&self, user_id: &str, context: &[u8]) -> Result<DerivedKey> {
        self.derive_user_key(user_id, context)
    }
}

/// Machine ID-based key derivation (for device binding without TPM)
pub struct MachineIdKeyDerivation {
    /// Machine ID bytes
    machine_id: Vec<u8>,
    /// Salt for Argon2
    salt: [u8; 16],
}

impl MachineIdKeyDerivation {
    /// Create from system machine ID
    pub fn from_system() -> Result<Self> {
        let machine_id = Self::read_machine_id()?;
        let mut salt = [0u8; 16];
        random_bytes(&mut salt);
        
        Ok(Self { machine_id, salt })
    }

    /// Create with specific machine ID and salt
    pub fn new(machine_id: Vec<u8>, salt: [u8; 16]) -> Self {
        Self { machine_id, salt }
    }

    /// Read machine ID from system
    fn read_machine_id() -> Result<Vec<u8>> {
        // Try /etc/machine-id first (systemd)
        if let Ok(id) = fs::read_to_string("/etc/machine-id") {
            return Ok(id.trim().as_bytes().to_vec());
        }

        // Try /var/lib/dbus/machine-id
        if let Ok(id) = fs::read_to_string("/var/lib/dbus/machine-id") {
            return Ok(id.trim().as_bytes().to_vec());
        }

        // Generate a random fallback
        let mut id = vec![0u8; 32];
        random_bytes(&mut id);
        Ok(id)
    }

    /// Get salt for storage
    #[must_use]
    pub fn salt(&self) -> &[u8; 16] {
        &self.salt
    }
}

impl KeyDerivation for MachineIdKeyDerivation {
    fn derive_key(&self, user_id: &str, context: &[u8]) -> Result<DerivedKey> {
        let mut input = self.machine_id.clone();
        input.extend_from_slice(user_id.as_bytes());
        input.extend_from_slice(context);

        let params = Params::new(32 * 1024, 2, 2, Some(KEY_SIZE)).expect("Valid params");
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        let mut key = [0u8; KEY_SIZE];
        argon2
            .hash_password_into(&input, &self.salt, &mut key)
            .map_err(|e| CryptoError::KeyDerivationFailed(e.to_string()))?;

        input.zeroize();

        Ok(DerivedKey::new(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derived_key_from_slice() {
        let bytes = [42u8; KEY_SIZE];
        let key = DerivedKey::from_slice(&bytes).unwrap();
        assert_eq!(key.as_bytes(), &bytes);
    }

    #[test]
    fn test_derived_key_invalid_length() {
        let bytes = [42u8; 16]; // Wrong size
        let result = DerivedKey::from_slice(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_derived_key_debug() {
        let key = DerivedKey::new([0u8; KEY_SIZE]);
        let debug = format!("{:?}", key);
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains("0, 0, 0")); // No actual key bytes
    }

    #[test]
    fn test_password_key_derivation() {
        let salt = [1u8; 16];
        let kdf = PasswordKeyDerivation::new(salt);
        
        let key1 = kdf.derive_from_password(b"password123").unwrap();
        let key2 = kdf.derive_from_password(b"password123").unwrap();
        let key3 = kdf.derive_from_password(b"different").unwrap();
        
        // Same password = same key
        assert_eq!(key1.as_bytes(), key2.as_bytes());
        
        // Different password = different key
        assert_ne!(key1.as_bytes(), key3.as_bytes());
    }

    #[test]
    fn test_password_key_derivation_different_salt() {
        let kdf1 = PasswordKeyDerivation::new([1u8; 16]);
        let kdf2 = PasswordKeyDerivation::new([2u8; 16]);
        
        let key1 = kdf1.derive_from_password(b"password").unwrap();
        let key2 = kdf2.derive_from_password(b"password").unwrap();
        
        // Same password, different salt = different key
        assert_ne!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_key_derivation_trait() {
        let kdf = PasswordKeyDerivation::with_random_salt();
        
        let key1 = kdf.derive_key("user1", b"context1").unwrap();
        let key2 = kdf.derive_key("user1", b"context1").unwrap();
        let key3 = kdf.derive_key("user2", b"context1").unwrap();
        
        assert_eq!(key1.as_bytes(), key2.as_bytes());
        assert_ne!(key1.as_bytes(), key3.as_bytes());
    }

    #[test]
    fn test_machine_id_derivation() {
        let machine_id = b"test-machine-id".to_vec();
        let salt = [42u8; 16];
        let kdf = MachineIdKeyDerivation::new(machine_id, salt);
        
        let key1 = kdf.derive_key("alice", b"template").unwrap();
        let key2 = kdf.derive_key("alice", b"template").unwrap();
        let key3 = kdf.derive_key("bob", b"template").unwrap();
        
        assert_eq!(key1.as_bytes(), key2.as_bytes());
        assert_ne!(key1.as_bytes(), key3.as_bytes());
    }
}
