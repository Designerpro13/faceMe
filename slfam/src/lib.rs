//! # Secure Lightweight Facial Authentication Module (SLFAM)
//!
//! A local, offline PAM-integrated facial authentication system with IR+RGB support,
//! multi-signal liveness detection, and TPM-bound encrypted templates.
//!
//! ## Features
//!
//! - **Multi-modal camera support**: RGB and IR cameras with automatic detection
//! - **Multi-signal liveness detection**: Blink detection (EAR), optical flow, LBP texture, IR reflectance
//! - **Secure template storage**: XChaCha20-Poly1305 encryption with TPM-bound keys
//! - **PAM integration**: Drop-in PAM module for Linux authentication
//! - **Offline operation**: No cloud dependencies, all processing is local
//!
//! ## Security Considerations
//!
//! - All sensitive data (embeddings, keys) are zeroized after use
//! - Templates are encrypted and bound to the device
//! - Rate limiting and lockout mechanisms prevent brute-force attacks
//! - Graceful fallback to password authentication on failure
//!
//! ## Modules
//!
//! - [`error`]: Error types for all SLFAM operations
//! - [`config`]: Configuration management
//! - [`camera`]: Camera abstraction layer
//! - [`detection`]: Face detection and landmark extraction
//! - [`liveness`]: Multi-signal liveness detection
//! - [`embedding`]: Face embedding generation
//! - [`crypto`]: Template encryption and key management
//! - [`matching`]: Embedding comparison and authentication
//! - [`template`]: Template storage and management
//! - [`pam`]: PAM module implementation

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod camera;
pub mod config;
pub mod crypto;
pub mod detection;
pub mod embedding;
pub mod error;
pub mod liveness;
pub mod matching;
pub mod pam;
pub mod template;

// Re-exports for convenience
pub use config::Config;
pub use error::{AuthError, Result};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Magic bytes for template files
pub const TEMPLATE_MAGIC: &[u8; 4] = b"SLFM";

/// Template format version
pub const TEMPLATE_VERSION: u8 = 1;

/// Default embedding dimension for MobileFaceNet
pub const EMBEDDING_DIM: usize = 512;

/// Prelude module for commonly used imports
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::error::{AuthError, Result};
    pub use crate::camera::{Camera, Frame};
    pub use crate::detection::{FaceDetectionPipeline, ProcessedFace, BoundingBox};
    pub use crate::embedding::{FaceEmbedding, EmbeddingGenerator};
    pub use crate::matching::{Matcher, MatchResult};
    pub use crate::template::{Template, TemplateStore};
}
