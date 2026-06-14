//! XChaCha20-Poly1305 authenticated encryption

use super::keys::DerivedKey;
use super::{generate_nonce, NONCE_SIZE, TAG_SIZE};
use crate::error::{CryptoError, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};

/// Encrypted data with nonce and tag
#[derive(Clone)]
pub struct EncryptedData {
    /// Nonce (24 bytes for XChaCha20)
    nonce: [u8; NONCE_SIZE],
    /// Ciphertext with authentication tag appended
    ciphertext: Vec<u8>,
}

impl EncryptedData {
    /// Create from components
    pub fn new(nonce: [u8; NONCE_SIZE], ciphertext: Vec<u8>) -> Self {
        Self { nonce, ciphertext }
    }

    /// Get the nonce
    #[must_use]
    pub fn nonce(&self) -> &[u8; NONCE_SIZE] {
        &self.nonce
    }

    /// Get the ciphertext (includes tag)
    #[must_use]
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }

    /// Serialize to bytes: nonce || ciphertext
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(NONCE_SIZE + self.ciphertext.len());
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&self.ciphertext);
        bytes
    }

    /// Deserialize from bytes
    ///
    /// # Errors
    ///
    /// Returns an error if data is too short
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < NONCE_SIZE + TAG_SIZE {
            return Err(CryptoError::DecryptionFailed("Data too short".to_string()).into());
        }

        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(&bytes[..NONCE_SIZE]);
        let ciphertext = bytes[NONCE_SIZE..].to_vec();

        Ok(Self { nonce, ciphertext })
    }

    /// Get total serialized length
    #[must_use]
    pub fn len(&self) -> usize {
        NONCE_SIZE + self.ciphertext.len()
    }

    /// Check if empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ciphertext.is_empty()
    }

    /// Get plaintext length (ciphertext - tag)
    #[must_use]
    pub fn plaintext_len(&self) -> usize {
        self.ciphertext.len().saturating_sub(TAG_SIZE)
    }
}

impl std::fmt::Debug for EncryptedData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptedData")
            .field("nonce_len", &NONCE_SIZE)
            .field("ciphertext_len", &self.ciphertext.len())
            .finish()
    }
}

/// Encrypt data using XChaCha20-Poly1305
///
/// # Arguments
///
/// * `key` - 256-bit encryption key
/// * `plaintext` - Data to encrypt
/// * `aad` - Additional authenticated data (optional, can be empty)
///
/// # Errors
///
/// Returns an error if encryption fails
pub fn encrypt(key: &DerivedKey, plaintext: &[u8], aad: &[u8]) -> Result<EncryptedData> {
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_bytes())
        .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

    let nonce = generate_nonce();
    let xnonce = XNonce::from_slice(&nonce);

    let ciphertext = if aad.is_empty() {
        cipher
            .encrypt(xnonce, plaintext)
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?
    } else {
        cipher
            .encrypt(
                xnonce,
                chacha20poly1305::aead::Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?
    };

    Ok(EncryptedData::new(nonce, ciphertext))
}

/// Decrypt data using XChaCha20-Poly1305
///
/// # Arguments
///
/// * `key` - 256-bit decryption key
/// * `encrypted` - Encrypted data with nonce
/// * `aad` - Additional authenticated data (must match encryption)
///
/// # Errors
///
/// Returns an error if decryption fails (wrong key, tampered data, etc.)
pub fn decrypt(key: &DerivedKey, encrypted: &EncryptedData, aad: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_bytes())
        .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;

    let xnonce = XNonce::from_slice(&encrypted.nonce);

    let plaintext = if aad.is_empty() {
        cipher
            .decrypt(xnonce, encrypted.ciphertext.as_ref())
            .map_err(|_| CryptoError::DecryptionFailed("Authentication failed".to_string()))?
    } else {
        cipher
            .decrypt(
                xnonce,
                chacha20poly1305::aead::Payload {
                    msg: encrypted.ciphertext.as_ref(),
                    aad,
                },
            )
            .map_err(|_| CryptoError::DecryptionFailed("Authentication failed".to_string()))?
    };

    Ok(plaintext)
}

/// Encrypt data and return raw bytes (nonce || ciphertext)
pub fn encrypt_to_bytes(key: &DerivedKey, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    let encrypted = encrypt(key, plaintext, aad)?;
    Ok(encrypted.to_bytes())
}

/// Decrypt raw bytes (nonce || ciphertext)
pub fn decrypt_from_bytes(key: &DerivedKey, data: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    let encrypted = EncryptedData::from_bytes(data)?;
    decrypt(key, &encrypted, aad)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> DerivedKey {
        DerivedKey::new([42u8; 32])
    }

    #[test]
    fn test_encrypt_decrypt() {
        let key = test_key();
        let plaintext = b"Hello, World!";
        
        let encrypted = encrypt(&key, plaintext, &[]).unwrap();
        let decrypted = decrypt(&key, &encrypted, &[]).unwrap();
        
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_with_aad() {
        let key = test_key();
        let plaintext = b"Secret message";
        let aad = b"Additional authenticated data";
        
        let encrypted = encrypt(&key, plaintext, aad).unwrap();
        let decrypted = decrypt(&key, &encrypted, aad).unwrap();
        
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_wrong_aad_fails() {
        let key = test_key();
        let plaintext = b"Secret";
        let aad = b"correct aad";
        let wrong_aad = b"wrong aad";
        
        let encrypted = encrypt(&key, plaintext, aad).unwrap();
        let result = decrypt(&key, &encrypted, wrong_aad);
        
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = DerivedKey::new([1u8; 32]);
        let key2 = DerivedKey::new([2u8; 32]);
        let plaintext = b"Secret";
        
        let encrypted = encrypt(&key1, plaintext, &[]).unwrap();
        let result = decrypt(&key2, &encrypted, &[]);
        
        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let key = test_key();
        let plaintext = b"Secret";
        
        let mut encrypted = encrypt(&key, plaintext, &[]).unwrap();
        
        // Tamper with ciphertext
        if !encrypted.ciphertext.is_empty() {
            encrypted.ciphertext[0] ^= 0xFF;
        }
        
        let result = decrypt(&key, &encrypted, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_serialization() {
        let key = test_key();
        let plaintext = b"Test data for serialization";
        
        let encrypted = encrypt(&key, plaintext, &[]).unwrap();
        let bytes = encrypted.to_bytes();
        let restored = EncryptedData::from_bytes(&bytes).unwrap();
        
        let decrypted = decrypt(&key, &restored, &[]).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_bytes() {
        let key = test_key();
        let plaintext = b"Round-trip test";
        
        let bytes = encrypt_to_bytes(&key, plaintext, &[]).unwrap();
        let decrypted = decrypt_from_bytes(&key, &bytes, &[]).unwrap();
        
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypted_data_len() {
        let encrypted = EncryptedData::new([0u8; NONCE_SIZE], vec![0u8; 100]);
        
        assert_eq!(encrypted.len(), NONCE_SIZE + 100);
        assert_eq!(encrypted.plaintext_len(), 100 - TAG_SIZE);
    }

    #[test]
    fn test_empty_plaintext() {
        let key = test_key();
        let plaintext = b"";
        
        let encrypted = encrypt(&key, plaintext, &[]).unwrap();
        let decrypted = decrypt(&key, &encrypted, &[]).unwrap();
        
        assert!(decrypted.is_empty());
    }

    #[test]
    fn test_large_plaintext() {
        let key = test_key();
        let plaintext = vec![0xAB; 1024 * 1024]; // 1 MiB
        
        let encrypted = encrypt(&key, &plaintext, &[]).unwrap();
        let decrypted = decrypt(&key, &encrypted, &[]).unwrap();
        
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_unique_nonces() {
        let key = test_key();
        let plaintext = b"Same message";
        
        let e1 = encrypt(&key, plaintext, &[]).unwrap();
        let e2 = encrypt(&key, plaintext, &[]).unwrap();
        
        // Nonces should be different
        assert_ne!(e1.nonce(), e2.nonce());
        
        // Ciphertexts should be different (due to different nonces)
        assert_ne!(e1.ciphertext(), e2.ciphertext());
    }

    #[test]
    fn test_from_bytes_too_short() {
        let short_data = vec![0u8; 10]; // Less than NONCE_SIZE + TAG_SIZE
        let result = EncryptedData::from_bytes(&short_data);
        assert!(result.is_err());
    }
}
