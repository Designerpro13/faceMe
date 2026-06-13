//! # Error Types for SLFAM
//!
//! This module defines comprehensive error types for all SLFAM operations.
//! Errors are categorized by subsystem for easier debugging and handling.

use std::path::PathBuf;
use thiserror::Error;

/// Result type alias for SLFAM operations
pub type Result<T> = std::result::Result<T, AuthError>;

/// Primary error type for authentication operations
#[derive(Error, Debug)]
pub enum AuthError {
    /// Camera-related errors
    #[error("Camera error: {0}")]
    Camera(#[from] CameraError),

    /// Face detection errors
    #[error("Detection error: {0}")]
    Detection(#[from] DetectionError),

    /// Liveness check errors
    #[error("Liveness error: {0}")]
    Liveness(#[from] LivenessError),

    /// Embedding generation errors
    #[error("Embedding error: {0}")]
    Embedding(#[from] EmbeddingError),

    /// Cryptographic errors
    #[error("Crypto error: {0}")]
    Crypto(#[from] CryptoError),

    /// Template storage errors
    #[error("Template error: {0}")]
    Template(#[from] TemplateError),

    /// Matching engine errors
    #[error("Matching error: {0}")]
    Matching(#[from] MatchingError),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    /// PAM-specific errors
    #[error("PAM error: {0}")]
    Pam(#[from] PamError),

    /// Rate limiting triggered
    #[error("Rate limited: too many attempts, locked for {lockout_seconds} seconds")]
    RateLimited {
        /// Seconds until lockout expires
        lockout_seconds: u64,
        /// Number of failed attempts
        failed_attempts: u32,
    },

    /// Generic I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Authentication failed (generic)
    #[error("Authentication failed")]
    AuthenticationFailed,

    /// Internal error (should not happen)
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Camera subsystem errors
#[derive(Error, Debug)]
pub enum CameraError {
    /// No camera device found
    #[error("No camera device found")]
    NoDevice,

    /// Camera device not found at specified path
    #[error("Camera device not found: {path}")]
    DeviceNotFound {
        /// Path to the device
        path: PathBuf,
    },

    /// Permission denied accessing camera
    #[error("Permission denied accessing camera: {path} (ensure user is in 'video' group)")]
    PermissionDenied {
        /// Path to the device
        path: PathBuf,
    },

    /// Camera device is busy (in use by another process)
    #[error("Camera device busy: {path} (close other applications using the camera)")]
    DeviceBusy {
        /// Path to the device
        path: PathBuf,
    },

    /// Failed to initialize camera
    #[error("Failed to initialize camera: {0}")]
    InitializationFailed(String),

    /// Failed to capture frame
    #[error("Failed to capture frame: {0}")]
    CaptureFailed(String),

    /// Frame capture timed out
    #[error("Frame capture timed out after {timeout_ms}ms")]
    CaptureTimeout {
        /// Timeout in milliseconds
        timeout_ms: u64,
    },

    /// Invalid frame format
    #[error("Invalid frame format: expected {expected}, got {actual}")]
    InvalidFormat {
        /// Expected format
        expected: String,
        /// Actual format received
        actual: String,
    },

    /// Camera capabilities not supported
    #[error("Camera capability not supported: {capability}")]
    UnsupportedCapability {
        /// The unsupported capability
        capability: String,
    },

    /// IR camera required but not found
    #[error("IR camera required but not available")]
    IrCameraRequired,

    /// V4L2 specific error
    #[error("V4L2 error: {0}")]
    V4l2(String),

    /// Feature not supported
    #[error("Camera feature not supported: {0}")]
    NotSupported(String),
}

/// Face detection errors
#[derive(Error, Debug)]
pub enum DetectionError {
    /// No face detected in frame
    #[error("No face detected in frame")]
    NoFaceDetected,

    /// Multiple faces detected when single face required
    #[error("Multiple faces detected ({count}), single face required")]
    MultipleFaces {
        /// Number of faces detected
        count: usize,
    },

    /// Face detection confidence too low
    #[error("Face detection confidence too low: {confidence:.2} < {threshold:.2}")]
    LowConfidence {
        /// Detected confidence
        confidence: f32,
        /// Required threshold
        threshold: f32,
    },

    /// Failed to load detection model
    #[error("Failed to load detection model from {path}: {reason}")]
    ModelLoadFailed {
        /// Path to the model
        path: PathBuf,
        /// Failure reason
        reason: String,
    },

    /// Face detection inference failed
    #[error("Detection inference failed: {0}")]
    InferenceFailed(String),

    /// Face too small in frame
    #[error("Face too small: {size}px, minimum {min_size}px required")]
    FaceTooSmall {
        /// Detected face size
        size: u32,
        /// Minimum required size
        min_size: u32,
    },

    /// Face too far from center
    #[error("Face not centered: distance from center {distance:.1}%")]
    FaceNotCentered {
        /// Distance from center as percentage
        distance: f32,
    },

    /// Landmark detection failed
    #[error("Landmark detection failed: {0}")]
    LandmarkFailed(String),

    /// Face alignment failed
    #[error("Face alignment failed: {0}")]
    AlignmentFailed(String),
}

/// Liveness detection errors
#[derive(Error, Debug)]
pub enum LivenessError {
    /// Liveness check failed (generic)
    #[error("Liveness check failed")]
    CheckFailed,

    /// Blink detection failed
    #[error("Blink not detected within {timeout_ms}ms")]
    BlinkNotDetected {
        /// Timeout for blink detection
        timeout_ms: u64,
    },

    /// Eye aspect ratio indicates closed eyes
    #[error("Eyes appear closed (EAR: {ear:.3})")]
    EyesClosed {
        /// Detected eye aspect ratio
        ear: f32,
    },

    /// Optical flow indicates flat surface (photo/screen)
    #[error("Suspected flat surface: optical flow variance {variance:.4} < {threshold:.4}")]
    FlatSurfaceDetected {
        /// Detected variance
        variance: f32,
        /// Required threshold
        threshold: f32,
    },

    /// LBP texture analysis indicates non-skin surface
    #[error("Texture analysis failed: suspected {surface_type}")]
    TextureCheckFailed {
        /// Type of surface detected
        surface_type: String,
    },

    /// IR reflectance indicates screen or photo
    #[error("IR reflectance check failed: {reason}")]
    IrCheckFailed {
        /// Reason for failure
        reason: String,
    },

    /// Challenge-response failed
    #[error("Challenge-response failed: {challenge} not completed")]
    ChallengeResponseFailed {
        /// The challenge that was not completed
        challenge: String,
    },

    /// Not enough frames for liveness analysis
    #[error("Insufficient frames for liveness check: {count} < {required}")]
    InsufficientFrames {
        /// Number of frames captured
        count: usize,
        /// Required number of frames
        required: usize,
    },
}

/// Embedding generation errors
#[derive(Error, Debug)]
pub enum EmbeddingError {
    /// Failed to load embedding model
    #[error("Failed to load embedding model from {path}: {reason}")]
    ModelLoadFailed {
        /// Path to the model
        path: PathBuf,
        /// Failure reason
        reason: String,
    },

    /// Embedding inference failed
    #[error("Embedding inference failed: {0}")]
    InferenceFailed(String),

    /// Invalid input dimensions
    #[error("Invalid input dimensions: expected {expected:?}, got {actual:?}")]
    InvalidInputDimensions {
        /// Expected dimensions
        expected: Vec<usize>,
        /// Actual dimensions
        actual: Vec<usize>,
    },

    /// Invalid embedding output
    #[error("Invalid embedding output: expected {expected} dimensions, got {actual}")]
    InvalidOutput {
        /// Expected dimension count
        expected: usize,
        /// Actual dimension count
        actual: usize,
    },

    /// Invalid embedding dimension
    #[error("Invalid embedding dimension: expected {expected}, got {got}")]
    InvalidDimension {
        expected: usize,
        got: usize,
    },

    /// Embedding generation failed
    #[error("Embedding generation failed: {0}")]
    GenerationFailed(String),

    /// Preprocessing failed
    #[error("Face preprocessing failed: {0}")]
    PreprocessingFailed(String),

    /// ONNX runtime error
    #[error("ONNX runtime error: {0}")]
    OnnxError(String),
}

/// Cryptographic errors
#[derive(Error, Debug)]
pub enum CryptoError {
    /// Key derivation failed
    #[error("Key derivation failed: {0}")]
    KeyDerivationFailed(String),

    /// Encryption failed
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),

    /// Decryption failed (wrong key or corrupted data)
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    /// Authentication tag verification failed
    #[error("Authentication tag verification failed: data has been tampered")]
    AuthenticationFailed,

    /// TPM error
    #[error("TPM error: {0}")]
    TpmError(String),

    /// TPM not available
    #[error("TPM not available, falling back to software key")]
    TpmNotAvailable,

    /// Key not found
    #[error("Encryption key not found")]
    KeyNotFound,

    /// Invalid key format
    #[error("Invalid key format: {0}")]
    InvalidKeyFormat(String),

    /// Invalid key length
    #[error("Invalid key length: expected {expected}, got {got}")]
    InvalidKeyLength {
        expected: usize,
        got: usize,
    },

    /// Nonce generation failed
    #[error("Nonce generation failed")]
    NonceGenerationFailed,

    /// sodiumoxide initialization failed
    #[error("Cryptographic library initialization failed")]
    InitializationFailed,
}

/// Template storage errors
#[derive(Error, Debug)]
pub enum TemplateError {
    /// Template not found for user
    #[error("Template not found for user: {0}")]
    NotFound(String),

    /// Template already exists
    #[error("Template already exists for user: {username}")]
    AlreadyExists {
        /// Username
        username: String,
    },

    /// Invalid template format
    #[error("Invalid template format: {0}")]
    InvalidFormat(String),

    /// Template version mismatch
    #[error("Template version mismatch: got {got}, max supported is {max_supported}")]
    UnsupportedVersion {
        /// Version in file
        got: u8,
        /// Maximum supported version
        max_supported: u8,
    },

    /// Invalid magic bytes
    #[error("Invalid template magic bytes")]
    InvalidMagic,

    /// Template corrupted
    #[error("Template file corrupted: {0}")]
    Corrupted(String),

    /// Failed to read template
    #[error("Failed to read template: {0}")]
    ReadFailed(String),

    /// Failed to write template
    #[error("Failed to write template: {0}")]
    WriteFailed(String),

    /// IO Error
    #[error("IO error: {0}")]
    IoError(String),

    /// Serialization error
    #[error("Serialization failed: {0}")]
    SerializationFailed(String),

    /// Template directory not found
    #[error("Template directory not found: {path}")]
    DirectoryNotFound {
        /// Path to directory
        path: PathBuf,
    },

    /// Permission error on template file
    #[error("Permission denied on template file: {path}")]
    PermissionDenied {
        /// Path to file
        path: PathBuf,
    },

    /// Model ID mismatch (template was created with different model)
    #[error("Template model mismatch: created with {template_model}, current model is {current_model}")]
    ModelMismatch {
        /// Model ID in template
        template_model: String,
        /// Current model ID
        current_model: String,
    },
}

/// Matching engine errors
#[derive(Error, Debug)]
pub enum MatchingError {
    /// Similarity score below threshold
    #[error("Similarity below threshold: {score:.4} < {threshold:.4}")]
    BelowThreshold {
        /// Computed similarity score
        score: f32,
        /// Required threshold
        threshold: f32,
    },

    /// Embedding dimension mismatch
    #[error("Embedding dimension mismatch: probe has {probe_dim}, gallery has {gallery_dim}")]
    DimensionMismatch {
        /// Probe embedding dimension
        probe_dim: usize,
        /// Gallery embedding dimension
        gallery_dim: usize,
    },

    /// Invalid embedding (contains NaN or Inf)
    #[error("Invalid embedding: {0}")]
    InvalidEmbedding(String),

    /// Empty embedding
    #[error("Empty embedding provided")]
    EmptyEmbedding,

    /// No templates available for matching
    #[error("No templates available for matching")]
    NoTemplates,

    /// Low quality score
    #[error("Quality score too low: {score:.2} < {threshold:.2}")]
    LowQuality {
        score: f32,
        threshold: f32,
    },
}

/// Configuration errors
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Configuration file not found
    #[error("Configuration file not found: {path}")]
    FileNotFound {
        /// Path to config file
        path: PathBuf,
    },

    /// Failed to load configuration
    #[error("Failed to load configuration: {0}")]
    LoadFailed(String),

    /// Failed to parse configuration
    #[error("Failed to parse configuration: {0}")]
    ParseFailed(String),

    /// Invalid configuration value
    #[error("Invalid configuration value for '{key}': {reason}")]
    InvalidValue {
        /// Configuration key
        key: String,
        /// Reason for invalidity
        reason: String,
    },

    /// Missing required configuration
    #[error("Missing required configuration: {key}")]
    MissingRequired {
        /// Missing key
        key: String,
    },

    /// Model directory not found
    #[error("Model directory not found: {path}")]
    ModelDirNotFound {
        /// Path to model directory
        path: PathBuf,
    },
}

/// PAM module errors
#[derive(Error, Debug)]
pub enum PamError {
    /// Failed to get username from PAM
    #[error("Failed to get username from PAM")]
    NoUsername,

    /// Invalid PAM handle
    #[error("Invalid PAM handle")]
    InvalidHandle,

    /// PAM conversation failed
    #[error("PAM conversation failed: {0}")]
    ConversationFailed(String),

    /// Service not allowed
    #[error("Service '{service}' not allowed for facial authentication")]
    ServiceNotAllowed {
        /// Service name
        service: String,
    },

    /// User not enrolled
    #[error("User '{0}' not enrolled for facial authentication")]
    UserNotEnrolled(String),

    /// Authentication failed
    #[error("Face authentication failed")]
    AuthenticationFailed,

    /// Authentication timed out
    #[error("Authentication timed out")]
    Timeout,

    /// No camera available
    #[error("No camera available for authentication")]
    NoCameraAvailable,

    /// Rate limited
    #[error("Too many authentication attempts, locked out for {lockout_seconds}s")]
    RateLimited {
        /// Lockout duration in seconds
        lockout_seconds: u64,
    },

    /// Authentication disabled
    #[error("Facial authentication is disabled")]
    Disabled,

    /// Emergency disable active
    #[error("Facial authentication emergency disabled (check /etc/slfam/disabled)")]
    EmergencyDisabled,
}

impl AuthError {
    /// Returns true if this error should trigger a fallback to password authentication
    #[must_use]
    pub fn should_fallback(&self) -> bool {
        matches!(
            self,
            AuthError::Camera(_)
                | AuthError::Detection(DetectionError::NoFaceDetected)
                | AuthError::Liveness(_)
                | AuthError::Matching(_)
                | AuthError::Template(TemplateError::NotFound(_))
                | AuthError::Pam(PamError::UserNotEnrolled(_))
                | AuthError::AuthenticationFailed
        )
    }

    /// Returns true if this error indicates a security concern
    #[must_use]
    pub fn is_security_concern(&self) -> bool {
        matches!(
            self,
            AuthError::Liveness(LivenessError::FlatSurfaceDetected { .. })
                | AuthError::Liveness(LivenessError::TextureCheckFailed { .. })
                | AuthError::Liveness(LivenessError::IrCheckFailed { .. })
                | AuthError::Crypto(CryptoError::AuthenticationFailed)
                | AuthError::Template(TemplateError::Corrupted(_))
                | AuthError::RateLimited { .. }
        )
    }

    /// Returns a sanitized error message suitable for logging (no sensitive data)
    #[must_use]
    pub fn sanitized_message(&self) -> String {
        match self {
            AuthError::Camera(_) => "Camera error".to_string(),
            AuthError::Detection(_) => "Detection error".to_string(),
            AuthError::Liveness(_) => "Liveness check error".to_string(),
            AuthError::Embedding(_) => "Embedding error".to_string(),
            AuthError::Crypto(_) => "Cryptographic error".to_string(),
            AuthError::Template(TemplateError::NotFound { .. }) => "Template not found".to_string(),
            AuthError::Template(_) => "Template error".to_string(),
            AuthError::Matching(_) => "Match failed".to_string(),
            AuthError::Config(_) => "Configuration error".to_string(),
            AuthError::Pam(_) => "PAM error".to_string(),
            AuthError::RateLimited { .. } => "Rate limited".to_string(),
            AuthError::Io(_) => "I/O error".to_string(),
            AuthError::AuthenticationFailed => "Authentication failed".to_string(),
            AuthError::Internal(_) => "Internal error".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_error_display() {
        let err = CameraError::DeviceNotFound {
            path: PathBuf::from("/dev/video0"),
        };
        assert!(err.to_string().contains("/dev/video0"));
    }

    #[test]
    fn test_auth_error_should_fallback() {
        let err = AuthError::Camera(CameraError::NoDevice);
        assert!(err.should_fallback());

        let err = AuthError::Crypto(CryptoError::DecryptionFailed);
        assert!(!err.should_fallback());
    }

    #[test]
    fn test_auth_error_is_security_concern() {
        let err = AuthError::Liveness(LivenessError::FlatSurfaceDetected {
            variance: 0.01,
            threshold: 0.05,
        });
        assert!(err.is_security_concern());

        let err = AuthError::Camera(CameraError::NoDevice);
        assert!(!err.is_security_concern());
    }

    #[test]
    fn test_sanitized_message_no_sensitive_data() {
        let err = AuthError::Template(TemplateError::NotFound {
            username: "sensitive_username".to_string(),
        });
        let msg = err.sanitized_message();
        assert!(!msg.contains("sensitive_username"));
    }

    #[test]
    fn test_error_conversion() {
        let camera_err = CameraError::NoDevice;
        let auth_err: AuthError = camera_err.into();
        assert!(matches!(auth_err, AuthError::Camera(_)));
    }
}
