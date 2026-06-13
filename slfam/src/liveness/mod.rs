//! # Liveness Detection Module
//!
//! Multi-signal liveness detection to prevent spoofing attacks.
//!
//! ## Supported Methods
//!
//! - **Blink Detection (EAR)**: Eye Aspect Ratio analysis
//! - **Optical Flow**: Motion analysis to detect flat surfaces
//! - **LBP Texture**: Local Binary Pattern texture analysis
//! - **IR Reflectance**: Infrared reflectance analysis (if IR camera available)

mod blink;
mod ir;
mod lbp;
mod optical_flow;

pub use blink::{BlinkDetector, BlinkState};
pub use ir::IrReflectanceAnalyzer;
pub use lbp::LbpTextureAnalyzer;
pub use optical_flow::OpticalFlowAnalyzer;

use crate::camera::Frame;
use crate::config::LivenessConfig;
use crate::detection::{FaceLandmarks, BoundingBox};
use crate::error::{LivenessError, Result};
use std::time::{Duration, Instant};

/// Result of liveness check
#[derive(Debug, Clone)]
pub struct LivenessResult {
    /// Whether the subject is determined to be live
    pub is_live: bool,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Individual check results
    pub checks: LivenessChecks,
    /// Time taken for analysis
    pub duration: Duration,
}

/// Individual liveness check results
#[derive(Debug, Clone, Default)]
pub struct LivenessChecks {
    /// Blink detection result
    pub blink: Option<CheckResult>,
    /// Optical flow result
    pub optical_flow: Option<CheckResult>,
    /// LBP texture result
    pub lbp: Option<CheckResult>,
    /// IR reflectance result
    pub ir: Option<CheckResult>,
}

/// Single check result
#[derive(Debug, Clone)]
pub struct CheckResult {
    /// Whether check passed
    pub passed: bool,
    /// Confidence/score for this check
    pub score: f32,
    /// Human-readable description
    pub description: String,
}

impl CheckResult {
    fn passed(score: f32, description: impl Into<String>) -> Self {
        Self {
            passed: true,
            score,
            description: description.into(),
        }
    }

    fn failed(score: f32, description: impl Into<String>) -> Self {
        Self {
            passed: false,
            score,
            description: description.into(),
        }
    }
}

/// Combined liveness analyzer
pub struct LivenessAnalyzer {
    /// Blink detector
    blink_detector: BlinkDetector,
    /// Optical flow analyzer
    optical_flow: OpticalFlowAnalyzer,
    /// LBP texture analyzer
    lbp_analyzer: LbpTextureAnalyzer,
    /// IR reflectance analyzer (optional)
    ir_analyzer: Option<IrReflectanceAnalyzer>,
    /// Configuration
    config: LivenessConfig,
    /// Frame buffer for temporal analysis
    frame_buffer: Vec<FrameData>,
    /// Maximum frames to buffer
    max_buffer_size: usize,
}

/// Stored frame data for analysis
struct FrameData {
    /// Grayscale frame data
    gray: Vec<u8>,
    /// Frame dimensions
    width: u32,
    height: u32,
    /// Landmarks for this frame
    landmarks: FaceLandmarks,
    /// Timestamp
    timestamp: Instant,
}

impl LivenessAnalyzer {
    /// Create a new liveness analyzer
    ///
    /// # Arguments
    ///
    /// * `config` - Liveness configuration
    /// * `has_ir` - Whether IR camera is available
    pub fn new(config: LivenessConfig, has_ir: bool) -> Self {
        let ir_analyzer = if has_ir && config.enable_ir_check {
            Some(IrReflectanceAnalyzer::new())
        } else {
            None
        };

        Self {
            blink_detector: BlinkDetector::new(
                config.ear_threshold,
                config.ear_consecutive_frames as usize,
            ),
            optical_flow: OpticalFlowAnalyzer::new(
                config.optical_flow_variance_threshold,
                config.optical_flow_frames as usize,
            ),
            lbp_analyzer: LbpTextureAnalyzer::new(),
            ir_analyzer,
            config,
            frame_buffer: Vec::new(),
            max_buffer_size: 30,
        }
    }

    /// Add a frame for analysis
    ///
    /// # Arguments
    ///
    /// * `frame` - RGB frame
    /// * `landmarks` - Detected landmarks for this frame
    pub fn add_frame(&mut self, frame: &Frame, landmarks: FaceLandmarks) -> Result<()> {
        let gray_frame = frame.to_grayscale()?;
        
        self.frame_buffer.push(FrameData {
            gray: gray_frame.into_data(),
            width: frame.width(),
            height: frame.height(),
            landmarks,
            timestamp: Instant::now(),
        });

        // Keep buffer size bounded
        while self.frame_buffer.len() > self.max_buffer_size {
            self.frame_buffer.remove(0);
        }

        Ok(())
    }

    /// Run liveness analysis on buffered frames
    ///
    /// # Errors
    ///
    /// Returns an error if insufficient frames or all checks fail
    pub fn analyze(&mut self) -> Result<LivenessResult> {
        let start = Instant::now();
        let mut checks = LivenessChecks::default();
        let mut passed_count = 0;
        let mut total_checks = 0;

        // Check minimum frames
        let min_frames = self.config.optical_flow_frames as usize;
        if self.frame_buffer.len() < min_frames {
            return Err(LivenessError::InsufficientFrames {
                count: self.frame_buffer.len(),
                required: min_frames,
            }
            .into());
        }

        // 1. Blink detection
        let blink_result = self.check_blink();
        if blink_result.passed {
            passed_count += 1;
        }
        checks.blink = Some(blink_result);
        total_checks += 1;

        // 2. Optical flow analysis
        let flow_result = self.check_optical_flow()?;
        if flow_result.passed {
            passed_count += 1;
        }
        checks.optical_flow = Some(flow_result);
        total_checks += 1;

        // 3. LBP texture analysis
        if self.config.enable_lbp {
            let lbp_result = self.check_lbp()?;
            if lbp_result.passed {
                passed_count += 1;
            }
            checks.lbp = Some(lbp_result);
            total_checks += 1;
        }

        // 4. IR reflectance (if available)
        if let Some(ref ir_analyzer) = self.ir_analyzer {
            // IR check would go here - requires IR frames
            // For now, skip if no IR data
        }

        // Determine final result
        let confidence = passed_count as f32 / total_checks as f32;
        let is_live = if self.config.strict_mode {
            // All checks must pass
            passed_count == total_checks
        } else {
            // Majority vote
            confidence >= 0.5
        };

        Ok(LivenessResult {
            is_live,
            confidence,
            checks,
            duration: start.elapsed(),
        })
    }

    /// Check blink detection across buffered frames
    fn check_blink(&mut self) -> CheckResult {
        self.blink_detector.reset();

        for frame_data in &self.frame_buffer {
            let ear = self.blink_detector.compute_ear(&frame_data.landmarks);
            self.blink_detector.update(ear);
        }

        let blink_detected = self.blink_detector.blink_detected();
        let score = if blink_detected { 1.0 } else { 0.0 };

        if blink_detected {
            CheckResult::passed(score, "Blink detected")
        } else {
            CheckResult::failed(score, "No blink detected")
        }
    }

    /// Check optical flow variance
    fn check_optical_flow(&self) -> Result<CheckResult> {
        if self.frame_buffer.len() < 2 {
            return Ok(CheckResult::failed(0.0, "Insufficient frames for optical flow"));
        }

        let variance = self.optical_flow.compute_variance(&self.frame_buffer)?;
        let threshold = self.config.optical_flow_variance_threshold;

        if variance >= threshold {
            Ok(CheckResult::passed(
                variance / threshold,
                format!("Motion variance: {:.4} (threshold: {:.4})", variance, threshold),
            ))
        } else {
            Ok(CheckResult::failed(
                variance / threshold,
                format!(
                    "Low motion variance: {:.4} (threshold: {:.4}) - possible flat surface",
                    variance, threshold
                ),
            ))
        }
    }

    /// Check LBP texture
    fn check_lbp(&self) -> Result<CheckResult> {
        if let Some(frame_data) = self.frame_buffer.last() {
            let (is_real, score) = self.lbp_analyzer.analyze(
                &frame_data.gray,
                frame_data.width as usize,
                frame_data.height as usize,
                &frame_data.landmarks,
            )?;

            if is_real {
                Ok(CheckResult::passed(score, "Texture appears to be real skin"))
            } else {
                Ok(CheckResult::failed(
                    score,
                    "Texture analysis indicates possible printed/screen image",
                ))
            }
        } else {
            Ok(CheckResult::failed(0.0, "No frames for LBP analysis"))
        }
    }

    /// Reset the analyzer state
    pub fn reset(&mut self) {
        self.frame_buffer.clear();
        self.blink_detector.reset();
    }

    /// Get number of buffered frames
    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.frame_buffer.len()
    }

    /// Check if we have enough frames for analysis
    #[must_use]
    pub fn ready_for_analysis(&self) -> bool {
        self.frame_buffer.len() >= self.config.optical_flow_frames as usize
    }
}

impl OpticalFlowAnalyzer {
    /// Compute variance from frame buffer
    fn compute_variance(&self, frames: &[FrameData]) -> Result<f32> {
        if frames.len() < 2 {
            return Ok(0.0);
        }

        let mut total_variance = 0.0;
        let mut count = 0;

        for i in 1..frames.len() {
            let prev = &frames[i - 1];
            let curr = &frames[i];

            if prev.width == curr.width && prev.height == curr.height {
                let variance = self.compute_flow_variance(
                    &prev.gray,
                    &curr.gray,
                    prev.width as usize,
                    prev.height as usize,
                    &curr.landmarks,
                );
                total_variance += variance;
                count += 1;
            }
        }

        if count > 0 {
            Ok(total_variance / count as f32)
        } else {
            Ok(0.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_result_creation() {
        let passed = CheckResult::passed(0.9, "Test passed");
        assert!(passed.passed);
        assert!((passed.score - 0.9).abs() < 0.001);

        let failed = CheckResult::failed(0.3, "Test failed");
        assert!(!failed.passed);
    }

    #[test]
    fn test_liveness_analyzer_creation() {
        let config = LivenessConfig::default();
        let analyzer = LivenessAnalyzer::new(config, false);
        assert_eq!(analyzer.frame_count(), 0);
        assert!(!analyzer.ready_for_analysis());
    }

    #[test]
    fn test_liveness_checks_default() {
        let checks = LivenessChecks::default();
        assert!(checks.blink.is_none());
        assert!(checks.optical_flow.is_none());
        assert!(checks.lbp.is_none());
        assert!(checks.ir.is_none());
    }
}
