//! # Configuration Module for SLFAM
//!
//! This module handles configuration loading, validation, and management.
//! Configuration can be loaded from a TOML file or environment variables.

use crate::error::{ConfigError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;

/// Default configuration file path
pub const DEFAULT_CONFIG_PATH: &str = "/etc/slfam/config.toml";

/// Default template directory
pub const DEFAULT_TEMPLATE_DIR: &str = "/var/lib/slfam/templates";

/// Default model directory
pub const DEFAULT_MODEL_DIR: &str = "/usr/share/slfam/models";

/// Default log file path
pub const DEFAULT_LOG_FILE: &str = "/var/log/slfam/audit.log";

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// General settings
    pub general: GeneralConfig,
    /// Camera settings
    pub camera: CameraConfig,
    /// Face detection settings
    pub detection: DetectionConfig,
    /// Liveness detection settings
    pub liveness: LivenessConfig,
    /// Matching settings
    pub matching: MatchingConfig,
    /// Security settings
    pub security: SecurityConfig,
    /// Enrollment settings
    pub enrollment: EnrollmentConfig,
}

/// General configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    /// Directory for storing encrypted templates
    pub template_dir: PathBuf,
    /// Directory containing ONNX models
    pub model_dir: PathBuf,
    /// Path to audit log file
    pub log_file: PathBuf,
    /// Log level (error, warn, info, debug, trace)
    pub log_level: String,
    /// Enable debug mode (for development only)
    pub debug_mode: bool,
}

/// Camera configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraConfig {
    /// RGB camera device ID (index or path)
    pub device_id: CameraDevice,
    /// IR camera device ID (optional)
    pub ir_device_id: Option<CameraDevice>,
    /// List of device IDs to try
    pub device_ids: Vec<u32>,
    /// Frame capture timeout in milliseconds
    pub capture_timeout_ms: u64,
    /// Camera access timeout in seconds
    pub timeout_secs: u64,
    /// Frame width
    pub frame_width: u32,
    /// Frame height
    pub frame_height: u32,
    /// Frames per second
    pub fps: u32,
    /// Whether to prefer IR camera when available
    pub prefer_ir: bool,
    /// Auto-detect cameras on startup
    pub auto_detect: bool,
}

/// Camera device identifier
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CameraDevice {
    /// Device index (e.g., 0 for /dev/video0)
    Index(u32),
    /// Device path (e.g., "/dev/video0")
    Path(String),
}

impl Default for CameraDevice {
    fn default() -> Self {
        CameraDevice::Index(0)
    }
}

impl CameraDevice {
    /// Convert to device path
    #[must_use]
    pub fn to_path(&self) -> PathBuf {
        match self {
            CameraDevice::Index(idx) => PathBuf::from(format!("/dev/video{}", idx)),
            CameraDevice::Path(path) => PathBuf::from(path),
        }
    }
}

/// Face detection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DetectionConfig {
    /// Face detection confidence threshold (0.0 - 1.0)
    pub confidence_threshold: f32,
    /// Maximum number of faces allowed (1 for authentication)
    pub max_faces: usize,
    /// Minimum face size in pixels
    pub min_face_size: u32,
    /// Detection model filename
    pub detection_model: String,
    /// Landmark model filename
    pub landmark_model: String,
    /// Embedding model filename
    pub embedding_model: String,
    /// Face alignment enabled
    pub enable_alignment: bool,
}

/// Liveness detection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LivenessConfig {
    /// Eye Aspect Ratio threshold for blink detection
    pub ear_threshold: f32,
    /// Number of consecutive frames with low EAR to confirm blink
    pub ear_consecutive_frames: u32,
    /// Optical flow variance threshold
    pub optical_flow_variance_threshold: f32,
    /// Number of frames to analyze for optical flow
    pub optical_flow_frames: u32,
    /// LBP texture analysis enabled
    pub enable_lbp: bool,
    /// IR reflectance check enabled (requires IR camera)
    pub enable_ir_check: bool,
    /// Require IR camera for authentication (fail if not available)
    pub require_ir: bool,
    /// Challenge-response enabled (random prompts)
    pub enable_challenge: bool,
    /// Liveness check timeout in milliseconds
    pub timeout_ms: u64,
    /// Strict mode: all checks must pass
    pub strict_mode: bool,
}

/// Matching configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MatchingConfig {
    /// Normal security threshold (0.0 - 1.0)
    pub normal_threshold: f32,
    /// High security threshold (0.0 - 1.0)
    pub high_security_threshold: f32,
    /// Use high security threshold
    pub use_high_security: bool,
    /// Embedding model filename
    pub embedding_model: String,
    /// Embedding dimension
    pub embedding_dim: usize,
    /// Score fusion strategy: "max", "average", "weighted"
    pub fusion_strategy: String,
    /// Minimum quality score for face samples
    pub min_quality_score: f32,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    /// Maximum failed attempts before lockout
    pub max_attempts: u32,
    /// Lockout duration in seconds
    pub lockout_duration_sec: u64,
    /// Enable rate limiting
    pub enable_rate_limiting: bool,
    /// Enable high security mode (stricter thresholds)
    pub high_security_mode: bool,
    /// Use TPM for key storage
    pub use_tpm: bool,
    /// TPM key handle (if using TPM)
    pub tpm_key_handle: Option<String>,
    /// Fallback key derivation iterations (if TPM unavailable)
    pub key_derivation_iterations: u32,
    /// Enable memory zeroization (should always be true in production)
    pub enable_zeroization: bool,
    /// Emergency disable file path
    pub emergency_disable_file: PathBuf,
}

/// Enrollment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EnrollmentConfig {
    /// Number of samples required for enrollment
    pub required_samples: usize,
    /// Interval between samples in seconds
    pub sample_interval_sec: u64,
    /// Require different head poses
    pub require_pose_variation: bool,
    /// Require different lighting conditions
    pub require_lighting_variation: bool,
    /// Maximum enrollment attempts
    pub max_enrollment_attempts: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            camera: CameraConfig::default(),
            detection: DetectionConfig::default(),
            liveness: LivenessConfig::default(),
            matching: MatchingConfig::default(),
            security: SecurityConfig::default(),
            enrollment: EnrollmentConfig::default(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            template_dir: PathBuf::from(DEFAULT_TEMPLATE_DIR),
            model_dir: PathBuf::from(DEFAULT_MODEL_DIR),
            log_file: PathBuf::from(DEFAULT_LOG_FILE),
            log_level: "info".to_string(),
            debug_mode: false,
        }
    }
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            device_id: CameraDevice::Index(0),
            ir_device_id: None,
            device_ids: vec![0, 1, 2],
            capture_timeout_ms: 5000,
            timeout_secs: 30,
            frame_width: 640,
            frame_height: 480,
            fps: 30,
            prefer_ir: true,
            auto_detect: true,
        }
    }
}

impl Default for DetectionConfig {
    fn default() -> Self {
        Self {
            confidence_threshold: 0.9,
            max_faces: 1,
            min_face_size: 80,
            detection_model: "retinaface.onnx".to_string(),
            landmark_model: "landmark_68.onnx".to_string(),
            embedding_model: "mobilefacenet.onnx".to_string(),
            enable_alignment: true,
        }
    }
}

impl Default for LivenessConfig {
    fn default() -> Self {
        Self {
            ear_threshold: 0.2,
            ear_consecutive_frames: 3,
            optical_flow_variance_threshold: 0.05,
            optical_flow_frames: 10,
            enable_lbp: true,
            enable_ir_check: false,
            require_ir: false,
            enable_challenge: false,
            timeout_ms: 3000,
            strict_mode: false,
        }
    }
}

impl Default for MatchingConfig {
    fn default() -> Self {
        Self {
            normal_threshold: 0.75,
            high_security_threshold: 0.85,
            use_high_security: false,
            embedding_model: "mobilefacenet.onnx".to_string(),
            embedding_dim: 512,
            fusion_strategy: "max".to_string(),
            min_quality_score: 0.5,
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            lockout_duration_sec: 900, // 15 minutes
            enable_rate_limiting: true,
            high_security_mode: false,
            use_tpm: false,
            tpm_key_handle: None,
            key_derivation_iterations: 100_000,
            enable_zeroization: true,
            emergency_disable_file: PathBuf::from("/etc/slfam/disabled"),
        }
    }
}

impl Default for EnrollmentConfig {
    fn default() -> Self {
        Self {
            required_samples: 5,
            sample_interval_sec: 1,
            require_pose_variation: true,
            require_lighting_variation: false,
            max_enrollment_attempts: 3,
        }
    }
}

impl Config {
    /// Load configuration from a file
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the TOML configuration file
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        
        if !path.exists() {
            return Err(ConfigError::FileNotFound {
                path: path.to_path_buf(),
            }.into());
        }

        let content = fs::read_to_string(path)
            .map_err(|e| ConfigError::ParseFailed(format!("Failed to read file: {}", e)))?;

        let config: Config = toml::from_str(&content)
            .map_err(|e| ConfigError::ParseFailed(e.to_string()))?;

        config.validate()?;
        Ok(config)
    }

    /// Load configuration from default path or create default config
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails
    pub fn load_or_default() -> Result<Self> {
        let default_path = Path::new(DEFAULT_CONFIG_PATH);
        
        if default_path.exists() {
            Self::load(default_path)
        } else {
            let config = Self::default();
            config.validate()?;
            Ok(config)
        }
    }

    /// Validate the configuration
    ///
    /// # Errors
    ///
    /// Returns an error if any configuration value is invalid
    pub fn validate(&self) -> Result<()> {
        // Validate thresholds
        if self.detection.confidence_threshold <= 0.0 || self.detection.confidence_threshold > 1.0 {
            return Err(ConfigError::InvalidValue {
                key: "detection.confidence_threshold".to_string(),
                reason: "Must be between 0.0 and 1.0".to_string(),
            }.into());
        }

        if self.matching.normal_threshold <= 0.0 || self.matching.normal_threshold > 1.0 {
            return Err(ConfigError::InvalidValue {
                key: "matching.normal_threshold".to_string(),
                reason: "Must be between 0.0 and 1.0".to_string(),
            }.into());
        }

        if self.matching.high_security_threshold <= 0.0 || self.matching.high_security_threshold > 1.0 {
            return Err(ConfigError::InvalidValue {
                key: "matching.high_security_threshold".to_string(),
                reason: "Must be between 0.0 and 1.0".to_string(),
            }.into());
        }

        if self.matching.high_security_threshold < self.matching.normal_threshold {
            return Err(ConfigError::InvalidValue {
                key: "matching.high_security_threshold".to_string(),
                reason: "Must be >= normal_threshold".to_string(),
            }.into());
        }

        if self.liveness.ear_threshold <= 0.0 || self.liveness.ear_threshold > 0.5 {
            return Err(ConfigError::InvalidValue {
                key: "liveness.ear_threshold".to_string(),
                reason: "Must be between 0.0 and 0.5".to_string(),
            }.into());
        }

        if self.security.max_attempts == 0 {
            return Err(ConfigError::InvalidValue {
                key: "security.max_attempts".to_string(),
                reason: "Must be > 0".to_string(),
            }.into());
        }

        if self.enrollment.required_samples == 0 {
            return Err(ConfigError::InvalidValue {
                key: "enrollment.required_samples".to_string(),
                reason: "Must be > 0".to_string(),
            }.into());
        }

        Ok(())
    }

    /// Get the effective matching threshold based on security mode
    #[must_use]
    pub fn effective_threshold(&self) -> f32 {
        if self.matching.use_high_security {
            self.matching.high_security_threshold
        } else {
            self.matching.normal_threshold
        }
    }

    /// Check if emergency disable is active
    #[must_use]
    pub fn is_emergency_disabled(&self) -> bool {
        self.security.emergency_disable_file.exists()
    }

    /// Create a development configuration with relaxed settings
    #[must_use]
    pub fn development() -> Self {
        Self {
            general: GeneralConfig {
                debug_mode: true,
                log_level: "debug".to_string(),
                ..GeneralConfig::default()
            },
            detection: DetectionConfig {
                confidence_threshold: 0.7,
                ..DetectionConfig::default()
            },
            liveness: LivenessConfig {
                ear_threshold: 0.25,
                optical_flow_variance_threshold: 0.03,
                strict_mode: false,
                ..LivenessConfig::default()
            },
            matching: MatchingConfig {
                normal_threshold: 0.65,
                high_security_threshold: 0.75,
                ..MatchingConfig::default()
            },
            security: SecurityConfig {
                max_attempts: 10,
                lockout_duration_sec: 60,
                ..SecurityConfig::default()
            },
            ..Self::default()
        }
    }

    /// Save configuration to a file
    ///
    /// # Arguments
    ///
    /// * `path` - Path to save the configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::ParseFailed(format!("Serialization failed: {}", e)))?;
        
        fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_default_config_is_valid() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_development_config_is_valid() {
        let config = Config::development();
        assert!(config.validate().is_ok());
        assert!(config.general.debug_mode);
        assert_eq!(config.matching.normal_threshold, 0.65);
    }

    #[test]
    fn test_invalid_threshold_validation() {
        let mut config = Config::default();
        config.detection.confidence_threshold = 1.5;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_high_security_threshold_validation() {
        let mut config = Config::default();
        config.matching.normal_threshold = 0.85;
        config.matching.high_security_threshold = 0.75;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_effective_threshold() {
        let mut config = Config::default();
        assert_eq!(config.effective_threshold(), config.matching.normal_threshold);
        
        config.matching.use_high_security = true;
        assert_eq!(config.effective_threshold(), config.matching.high_security_threshold);
    }

    #[test]
    fn test_config_save_and_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_config.toml");
        
        let config = Config::default();
        config.save(&path).unwrap();
        
        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.matching.normal_threshold, config.matching.normal_threshold);
    }

    #[test]
    fn test_camera_device_to_path() {
        let device = CameraDevice::Index(0);
        assert_eq!(device.to_path(), PathBuf::from("/dev/video0"));
        
        let device = CameraDevice::Path("/dev/video2".to_string());
        assert_eq!(device.to_path(), PathBuf::from("/dev/video2"));
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = Config::load("/nonexistent/path/config.toml");
        assert!(result.is_err());
    }
}
