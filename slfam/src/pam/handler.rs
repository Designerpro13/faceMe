//! PAM authentication handler

use crate::camera::Camera;
use crate::config::Config;
use crate::crypto::{KeyDerivation, TpmKeyDerivation};
use crate::detection::FaceDetectionPipeline;
use crate::embedding::EmbeddingGenerator;
use crate::error::{PamError, Result};
use crate::liveness::LivenessAnalyzer;
use crate::matching::{Matcher, SecurityLevel};
use crate::template::TemplateStore;
use std::time::{Duration, Instant};

use super::PamArgs;

/// Authentication handler for PAM
pub struct PamHandler {
    /// Configuration
    config: Config,
    /// Module arguments
    args: PamArgs,
    /// Template store
    template_store: Option<TemplateStore>,
    /// Key derivation
    key_derivation: Option<TpmKeyDerivation>,
}

impl PamHandler {
    /// Create a new PAM handler
    pub fn new(config: Config, args: PamArgs) -> Result<Self> {
        Ok(Self {
            config,
            args,
            template_store: None,
            key_derivation: None,
        })
    }

    /// Initialize resources (lazy initialization)
    fn initialize(&mut self) -> Result<()> {
        if self.template_store.is_none() {
            self.template_store = Some(TemplateStore::new(&self.config.general.template_dir)?);
        }

        if self.key_derivation.is_none() {
            let key_path = std::path::Path::new(&self.config.general.template_dir).join(".key");
            self.key_derivation = Some(TpmKeyDerivation::new(
                &key_path,
                self.config.security.use_tpm,
            )?);
        }

        Ok(())
    }

    /// Perform facial authentication
    pub fn authenticate(&mut self, username: &str) -> Result<()> {
        let start = Instant::now();
        let timeout = Duration::from_secs(
            self.args.timeout.unwrap_or(self.config.camera.timeout_secs as u32) as u64
        );

        // Initialize resources
        self.initialize()?;

        let template_store = self.template_store.as_mut().unwrap();
        let key_derivation = self.key_derivation.as_ref().unwrap();

        // Check if user has a template
        if !template_store.exists(username) {
            return Err(PamError::UserNotEnrolled(username.to_string()).into());
        }

        // Derive user key and load template
        let key = key_derivation.derive_key(username, b"slfam-auth")?;
        let template = template_store.load(username, &key)?;

        if template.embeddings().is_empty() {
            return Err(PamError::UserNotEnrolled(username.to_string()).into());
        }

        // Initialize camera
        let mut camera = self.open_camera()?;

        // Initialize face detection
        let detection_pipeline = FaceDetectionPipeline::new(
            &self.config.general.model_dir,
            self.config.detection.clone(),
        )?;

        // Initialize liveness analyzer
        let has_ir = camera.is_ir();
        let mut liveness = LivenessAnalyzer::new(self.config.liveness.clone(), has_ir);

        // Initialize embedding generator
        let embedding_model = std::path::Path::new(&self.config.general.model_dir)
            .join(&self.config.detection.embedding_model);
        let embedding_gen = EmbeddingGenerator::load(&embedding_model)?;

        // Initialize matcher
        let mut matcher = Matcher::new(self.config.matching.clone());
        if self.config.security.high_security_mode {
            matcher.set_security_level(SecurityLevel::High);
        }

        // Start camera
        camera.start_streaming()?;

        // Capture and process frames
        let required_frames = self.config.liveness.optical_flow_frames as usize;
        let mut attempts = 0;
        let max_attempts = 3;

        while attempts < max_attempts {
            attempts += 1;

            // Check timeout
            if start.elapsed() > timeout {
                return Err(PamError::Timeout.into());
            }

            // Collect frames for liveness
            liveness.reset();
            let mut last_face = None;

            for _ in 0..required_frames {
                if start.elapsed() > timeout {
                    return Err(PamError::Timeout.into());
                }

                // Capture frame
                let frame = camera.capture_frame()?;

                // Detect face
                match detection_pipeline.process_frame(&frame) {
                    Ok(processed) => {
                        liveness.add_frame(&frame, processed.landmarks.clone())?;
                        last_face = Some(processed);
                    }
                    Err(e) => {
                        if self.args.debug {
                            eprintln!("slfam: frame processing error: {}", e);
                        }
                        continue;
                    }
                }
            }

            // Run liveness check (unless disabled)
            if !self.args.skip_liveness {
                match liveness.analyze() {
                    Ok(result) if result.is_live => {
                        if self.args.debug {
                            eprintln!("slfam: liveness passed (confidence: {:.2})", result.confidence);
                        }
                    }
                    Ok(result) => {
                        if self.args.debug {
                            eprintln!("slfam: liveness failed (confidence: {:.2})", result.confidence);
                        }
                        continue;
                    }
                    Err(e) => {
                        if self.args.debug {
                            eprintln!("slfam: liveness error: {}", e);
                        }
                        continue;
                    }
                }
            }

            // Generate embedding from last good face
            let processed = last_face.ok_or_else(|| PamError::AuthenticationFailed)?;
            let aligned = processed.aligned.ok_or_else(|| PamError::AuthenticationFailed)?;
            let probe_embedding = embedding_gen.generate(&aligned)?;

            // Match against enrolled templates
            let match_result = matcher.match_many(&probe_embedding, template.embeddings())?;

            if match_result.matched {
                if self.args.debug {
                    eprintln!(
                        "slfam: authentication successful (similarity: {:.4}, threshold: {:.4})",
                        match_result.similarity, match_result.threshold
                    );
                }
                return Ok(());
            } else {
                if self.args.debug {
                    eprintln!(
                        "slfam: match failed (similarity: {:.4}, threshold: {:.4})",
                        match_result.similarity, match_result.threshold
                    );
                }
            }
        }

        Err(PamError::AuthenticationFailed.into())
    }

    /// Open camera with retry logic
    fn open_camera(&self) -> Result<Camera> {
        use crate::camera::CameraType;
        
        // Try to open default camera
        Camera::open(&self.config.camera, CameraType::Rgb)
            .map_err(|_| PamError::NoCameraAvailable.into())
    }
}

/// Rate limiter for authentication attempts
pub struct RateLimiter {
    /// User -> (attempt count, first attempt time)
    attempts: std::collections::HashMap<String, (u32, Instant)>,
    /// Maximum attempts in window
    max_attempts: u32,
    /// Window duration
    window: Duration,
    /// Lockout duration after max attempts
    lockout: Duration,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(max_attempts: u32, window_secs: u64, lockout_secs: u64) -> Self {
        Self {
            attempts: std::collections::HashMap::new(),
            max_attempts,
            window: Duration::from_secs(window_secs),
            lockout: Duration::from_secs(lockout_secs),
        }
    }

    /// Check if user is allowed to attempt authentication
    pub fn check(&mut self, user: &str) -> Result<()> {
        let now = Instant::now();

        if let Some((count, first_time)) = self.attempts.get(user) {
            let elapsed = now.duration_since(*first_time);

            // Check if still in lockout
            if *count >= self.max_attempts {
                if elapsed < self.lockout {
                    let remaining = self.lockout - elapsed;
                    return Err(PamError::RateLimited {
                        lockout_seconds: remaining.as_secs(),
                    }
                    .into());
                } else {
                    // Lockout expired, reset
                    self.attempts.remove(user);
                }
            } else if elapsed >= self.window {
                // Window expired, reset
                self.attempts.remove(user);
            }
        }

        Ok(())
    }

    /// Record an authentication attempt
    pub fn record_attempt(&mut self, user: &str, success: bool) {
        if success {
            self.attempts.remove(user);
        } else {
            let now = Instant::now();
            let entry = self.attempts.entry(user.to_string()).or_insert((0, now));
            entry.0 += 1;
        }
    }

    /// Clear all attempts for a user
    pub fn clear(&mut self, user: &str) {
        self.attempts.remove(user);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter() {
        let mut limiter = RateLimiter::new(3, 60, 300);

        // First few attempts should pass
        assert!(limiter.check("user1").is_ok());
        limiter.record_attempt("user1", false);
        
        assert!(limiter.check("user1").is_ok());
        limiter.record_attempt("user1", false);
        
        assert!(limiter.check("user1").is_ok());
        limiter.record_attempt("user1", false);

        // Fourth attempt should be rate limited
        assert!(limiter.check("user1").is_err());

        // Different user should be fine
        assert!(limiter.check("user2").is_ok());
    }

    #[test]
    fn test_rate_limiter_success_clears() {
        let mut limiter = RateLimiter::new(3, 60, 300);

        limiter.record_attempt("user1", false);
        limiter.record_attempt("user1", false);
        limiter.record_attempt("user1", true); // Success clears

        // Should be able to try again
        assert!(limiter.check("user1").is_ok());
    }
}
