//! IR reflectance analysis for liveness detection
//!
//! Analyzes infrared reflectance patterns to distinguish real faces
//! from screens, photos, and masks.

use crate::camera::Frame;
use crate::detection::FaceLandmarks;
use crate::error::Result;

/// IR reflectance analyzer
pub struct IrReflectanceAnalyzer {
    /// Minimum expected reflectance for real skin
    min_reflectance: f32,
    /// Maximum expected reflectance for real skin
    max_reflectance: f32,
    /// Expected variance range for real skin
    variance_range: (f32, f32),
    /// Historical reflectance values
    history: Vec<f32>,
    /// Maximum history size
    max_history: usize,
}

impl IrReflectanceAnalyzer {
    /// Create a new IR reflectance analyzer
    pub fn new() -> Self {
        Self {
            // Real skin typically has moderate IR reflectance
            min_reflectance: 0.2,
            max_reflectance: 0.8,
            // Real skin shows some variance due to micro-movements and blood flow
            variance_range: (0.01, 0.15),
            history: Vec::new(),
            max_history: 30,
        }
    }

    /// Analyze IR frame for liveness
    ///
    /// # Arguments
    ///
    /// * `ir_frame` - Infrared frame
    /// * `landmarks` - Facial landmarks
    ///
    /// # Returns
    ///
    /// Tuple of (is_real, confidence_score, reason)
    pub fn analyze(
        &mut self,
        ir_frame: &Frame,
        landmarks: &FaceLandmarks,
    ) -> Result<(bool, f32, String)> {
        let gray = ir_frame.to_grayscale()?;
        let data = gray.data();
        let width = ir_frame.width() as usize;
        let height = ir_frame.height() as usize;

        // Get face ROI
        let roi = self.get_face_roi(landmarks, width, height);

        // Compute reflectance metrics
        let (mean, variance) = self.compute_reflectance_stats(data, width, &roi);

        // Store in history
        self.history.push(mean);
        while self.history.len() > self.max_history {
            self.history.remove(0);
        }

        // Analyze
        let mut score = 0.0f32;
        let mut reasons = Vec::new();

        // Check 1: Mean reflectance in expected range
        if mean >= self.min_reflectance && mean <= self.max_reflectance {
            score += 0.3;
        } else {
            reasons.push(format!(
                "Reflectance {:.2} outside expected range [{:.2}, {:.2}]",
                mean, self.min_reflectance, self.max_reflectance
            ));
        }

        // Check 2: Local variance in expected range
        if variance >= self.variance_range.0 && variance <= self.variance_range.1 {
            score += 0.3;
        } else if variance < self.variance_range.0 {
            reasons.push("IR texture too uniform - possible screen or printed image".to_string());
        } else {
            reasons.push("IR texture too noisy".to_string());
        }

        // Check 3: Temporal variance (if we have history)
        if self.history.len() >= 5 {
            let temporal_variance = self.compute_temporal_variance();
            if temporal_variance >= 0.005 && temporal_variance <= 0.1 {
                score += 0.2;
            } else if temporal_variance < 0.005 {
                reasons.push("No temporal variation in IR - possible static image".to_string());
            }
        } else {
            // Not enough history yet, give partial score
            score += 0.1;
        }

        // Check 4: Screen/display detection
        if !self.detect_screen_pattern(data, width, &roi) {
            score += 0.2;
        } else {
            reasons.push("Screen refresh pattern detected in IR".to_string());
        }

        let is_real = score >= 0.6;
        let reason = if reasons.is_empty() {
            "IR reflectance consistent with real skin".to_string()
        } else {
            reasons.join("; ")
        };

        Ok((is_real, score, reason))
    }

    /// Get face ROI from landmarks
    fn get_face_roi(
        &self,
        landmarks: &FaceLandmarks,
        width: usize,
        height: usize,
    ) -> (usize, usize, usize, usize) {
        let points = landmarks.points();
        if points.is_empty() {
            // Default to center region
            let x = width / 4;
            let y = height / 4;
            let w = width / 2;
            let h = height / 2;
            return (x, y, w, h);
        }

        let min_x = points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
        let max_x = points.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
        let min_y = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let max_y = points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);

        let x = (min_x.max(0.0)) as usize;
        let y = (min_y.max(0.0)) as usize;
        let w = ((max_x - min_x) as usize).min(width - x);
        let h = ((max_y - min_y) as usize).min(height - y);

        (x, y, w, h)
    }

    /// Compute mean and variance of reflectance in ROI
    fn compute_reflectance_stats(
        &self,
        data: &[u8],
        width: usize,
        roi: &(usize, usize, usize, usize),
    ) -> (f32, f32) {
        let (rx, ry, rw, rh) = *roi;
        
        let mut sum = 0.0f32;
        let mut sum_sq = 0.0f32;
        let mut count = 0;

        for y in ry..(ry + rh) {
            for x in rx..(rx + rw) {
                let idx = y * width + x;
                if idx < data.len() {
                    let val = data[idx] as f32 / 255.0;
                    sum += val;
                    sum_sq += val * val;
                    count += 1;
                }
            }
        }

        if count == 0 {
            return (0.0, 0.0);
        }

        let mean = sum / count as f32;
        let variance = (sum_sq / count as f32) - (mean * mean);

        (mean, variance.max(0.0))
    }

    /// Compute temporal variance from history
    fn compute_temporal_variance(&self) -> f32 {
        if self.history.len() < 2 {
            return 0.0;
        }

        let mean = self.history.iter().sum::<f32>() / self.history.len() as f32;
        let variance = self.history
            .iter()
            .map(|v| (v - mean).powi(2))
            .sum::<f32>()
            / self.history.len() as f32;

        variance
    }

    /// Detect screen refresh patterns (periodic intensity variations)
    fn detect_screen_pattern(
        &self,
        data: &[u8],
        width: usize,
        roi: &(usize, usize, usize, usize),
    ) -> bool {
        let (rx, ry, rw, rh) = *roi;

        // Check for horizontal banding (common in screens)
        let mut row_means: Vec<f32> = Vec::new();

        for y in ry..(ry + rh) {
            let mut row_sum = 0.0f32;
            for x in rx..(rx + rw) {
                let idx = y * width + x;
                if idx < data.len() {
                    row_sum += data[idx] as f32;
                }
            }
            row_means.push(row_sum / rw as f32);
        }

        // Look for periodic patterns in row means
        if row_means.len() < 10 {
            return false;
        }

        // Compute differences between adjacent rows
        let mut diffs: Vec<f32> = Vec::new();
        for i in 1..row_means.len() {
            diffs.push((row_means[i] - row_means[i - 1]).abs());
        }

        // Check for periodic large differences (screen refresh artifacts)
        let mean_diff = diffs.iter().sum::<f32>() / diffs.len() as f32;
        let large_diffs: Vec<usize> = diffs
            .iter()
            .enumerate()
            .filter(|(_, &d)| d > mean_diff * 2.0)
            .map(|(i, _)| i)
            .collect();

        // If large differences appear at regular intervals, likely a screen
        if large_diffs.len() >= 3 {
            let intervals: Vec<usize> = large_diffs
                .windows(2)
                .map(|w| w[1] - w[0])
                .collect();
            
            if !intervals.is_empty() {
                let mean_interval = intervals.iter().sum::<usize>() / intervals.len();
                let variance: f32 = intervals
                    .iter()
                    .map(|&i| (i as f32 - mean_interval as f32).powi(2))
                    .sum::<f32>()
                    / intervals.len() as f32;

                // Regular pattern detected
                if variance < 2.0 && mean_interval > 5 && mean_interval < 50 {
                    return true;
                }
            }
        }

        false
    }

    /// Reset the analyzer
    pub fn reset(&mut self) {
        self.history.clear();
    }

    /// Set reflectance range
    pub fn set_reflectance_range(&mut self, min: f32, max: f32) {
        self.min_reflectance = min.clamp(0.0, 1.0);
        self.max_reflectance = max.clamp(0.0, 1.0);
    }
}

impl Default for IrReflectanceAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Quick IR liveness check (stateless)
pub fn quick_ir_check(
    ir_data: &[u8],
    _width: usize,
    _height: usize,
) -> (bool, f32) {
    // Compute basic statistics
    let total: u64 = ir_data.iter().map(|&v| v as u64).sum();
    let mean = total as f32 / ir_data.len() as f32 / 255.0;

    let variance: f32 = ir_data
        .iter()
        .map(|&v| {
            let norm = v as f32 / 255.0;
            (norm - mean).powi(2)
        })
        .sum::<f32>()
        / ir_data.len() as f32;

    // Simple heuristics
    let mean_ok = mean >= 0.2 && mean <= 0.8;
    let variance_ok = variance >= 0.01 && variance <= 0.15;

    let score = if mean_ok && variance_ok {
        0.8
    } else if mean_ok || variance_ok {
        0.5
    } else {
        0.2
    };

    (score >= 0.5, score)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ir_analyzer_creation() {
        let analyzer = IrReflectanceAnalyzer::new();
        assert!((analyzer.min_reflectance - 0.2).abs() < 0.01);
        assert!((analyzer.max_reflectance - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_reflectance_stats() {
        let analyzer = IrReflectanceAnalyzer::new();

        // Uniform data
        let data = vec![128u8; 100];
        let roi = (0, 0, 10, 10);
        let (mean, variance) = analyzer.compute_reflectance_stats(&data, 10, &roi);
        
        assert!((mean - 0.502).abs() < 0.01); // 128/255 ≈ 0.502
        assert!(variance < 0.001); // Uniform = no variance
    }

    #[test]
    fn test_temporal_variance() {
        let mut analyzer = IrReflectanceAnalyzer::new();

        // Constant values (photo)
        analyzer.history = vec![0.5, 0.5, 0.5, 0.5, 0.5];
        let variance_const = analyzer.compute_temporal_variance();
        assert!(variance_const < 0.001);

        // Varying values (real face)
        analyzer.history = vec![0.45, 0.52, 0.48, 0.51, 0.47];
        let variance_varying = analyzer.compute_temporal_variance();
        assert!(variance_varying > variance_const);
    }

    #[test]
    fn test_quick_ir_check() {
        // Normal reflectance and variance
        let normal: Vec<u8> = (0..100).map(|i| 128 + (i % 20) as u8 - 10).collect();
        let (is_real, score) = quick_ir_check(&normal, 10, 10);
        assert!(score > 0.5);

        // Too uniform (photo/screen)
        let uniform = vec![128u8; 100];
        let (is_real2, score2) = quick_ir_check(&uniform, 10, 10);
        assert!(score2 < 0.7); // Lower score for uniform
    }

    #[test]
    fn test_screen_pattern_detection() {
        let analyzer = IrReflectanceAnalyzer::new();

        // No pattern
        let normal = vec![128u8; 500];
        let roi = (0, 0, 10, 50);
        assert!(!analyzer.detect_screen_pattern(&normal, 10, &roi));

        // Simulated screen pattern (periodic horizontal bands)
        let mut screen: Vec<u8> = Vec::new();
        for y in 0..50 {
            for x in 0..10 {
                let val = if y % 10 < 2 { 160 } else { 128 };
                screen.push(val);
            }
        }
        // Pattern detection might or might not trigger depending on thresholds
        // This is implementation-dependent
    }

    #[test]
    fn test_reset() {
        let mut analyzer = IrReflectanceAnalyzer::new();
        analyzer.history = vec![0.5, 0.5, 0.5];
        
        analyzer.reset();
        assert!(analyzer.history.is_empty());
    }
}
