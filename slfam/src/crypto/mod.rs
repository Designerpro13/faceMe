//! # Cryptographic Module
//!
//! Provides encryption, key derivation, and secure storage for face templates.
//!
//! ## Features
//!
//! - XChaCha20-Poly1305 authenticated encryption
//! - TPM-bound key derivation (when available)
//! - Argon2id password-based key derivation
//! - Secure random number generation

mod keys;
mod xchacha;

pub use keys::{DerivedKey, KeyDerivation, TpmKeyDerivation};
pub use xchacha::{decrypt, encrypt, EncryptedData};

use crate::error::Result;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use zeroize::Zeroize;

/// Size of encryption key in bytes (256 bits)
pub const KEY_SIZE: usize = 32;

/// Size of XChaCha20 nonce in bytes
pub const NONCE_SIZE: usize = 24;

/// Size of Poly1305 authentication tag
pub const TAG_SIZE: usize = 16;

/// Generate cryptographically secure random bytes
///
/// # Arguments
///
/// * `output` - Buffer to fill with random bytes
pub fn random_bytes(output: &mut [u8]) {
    let mut rng = ChaCha20Rng::from_entropy();
    rng.fill_bytes(output);
}

/// Generate a random nonce
#[must_use]
pub fn generate_nonce() -> [u8; NONCE_SIZE] {
    let mut nonce = [0u8; NONCE_SIZE];
    random_bytes(&mut nonce);
    nonce
}

/// Generate a random key
#[must_use]
pub fn generate_key() -> DerivedKey {
    let mut key_bytes = [0u8; KEY_SIZE];
    random_bytes(&mut key_bytes);
    DerivedKey::new(key_bytes)
}

/// Securely compare two byte slices in constant time
///
/// # Arguments
///
/// * `a` - First slice
/// * `b` - Second slice
///
/// # Returns
///
/// True if slices are equal
#[must_use]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Securely zero memory
pub fn secure_zero(data: &mut [u8]) {
    data.zeroize();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_bytes() {
        let mut buf1 = [0u8; 32];
        let mut buf2 = [0u8; 32];
        
        random_bytes(&mut buf1);
        random_bytes(&mut buf2);
        
        // Should be different (with overwhelming probability)
        assert_ne!(buf1, buf2);
        
        // Should not be all zeros
        assert!(buf1.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_generate_nonce() {
        let nonce1 = generate_nonce();
        let nonce2 = generate_nonce();
        
        assert_eq!(nonce1.len(), NONCE_SIZE);
        assert_ne!(nonce1, nonce2);
    }

    #[test]
    fn test_constant_time_eq() {
        let a = [1u8, 2, 3, 4];
        let b = [1u8, 2, 3, 4];
        let c = [1u8, 2, 3, 5];
        let d = [1u8, 2, 3];
        
        assert!(constant_time_eq(&a, &b));
        assert!(!constant_time_eq(&a, &c));
        assert!(!constant_time_eq(&a, &d));
    }

    #[test]
    fn test_secure_zero() {
        let mut data = [1u8, 2, 3, 4, 5];
        secure_zero(&mut data);
        assert_eq!(data, [0u8; 5]);
    }
}
