//! Optical flow analysis for liveness detection
//!
//! Computes motion variance across face regions to distinguish
//! live faces from flat surfaces (photos, screens).

use crate::detection::{FaceLandmarks, BoundingBox};
use crate::error::Result;

/// Optical flow analyzer using Lucas-Kanade-like approach
pub struct OpticalFlowAnalyzer {
    /// Variance threshold for liveness
    variance_threshold: f32,
    /// Number of frames to analyze
    frame_count: usize,
    /// Block size for flow computation
    block_size: usize,
    /// Search window size
    window_size: usize,
}

impl OpticalFlowAnalyzer {
    /// Create a new optical flow analyzer
    ///
    /// # Arguments
    ///
    /// * `variance_threshold` - Minimum variance to consider as live (typically 0.05)
    /// * `frame_count` - Number of frames to analyze
    pub fn new(variance_threshold: f32, frame_count: usize) -> Self {
        Self {
            variance_threshold,
            frame_count,
            block_size: 8,
            window_size: 15,
        }
    }

    /// Compute optical flow variance between two frames
    ///
    /// # Arguments
    ///
    /// * `prev_gray` - Previous frame (grayscale)
    /// * `curr_gray` - Current frame (grayscale)
    /// * `width` - Frame width
    /// * `height` - Frame height
    /// * `landmarks` - Facial landmarks for ROI extraction
    ///
    /// # Returns
    ///
    /// Flow variance across face regions
    pub fn compute_flow_variance(
        &self,
        prev_gray: &[u8],
        curr_gray: &[u8],
        width: usize,
        height: usize,
        landmarks: &FaceLandmarks,
    ) -> f32 {
        // Define ROIs: left cheek, nose, right cheek
        let rois = self.get_face_rois(landmarks, width, height);

        if rois.is_empty() {
            return 0.0;
        }

        // Compute flow in each ROI
        let mut roi_flows: Vec<(f32, f32)> = Vec::new();

        for roi in &rois {
            if let Some(flow) = self.compute_roi_flow(prev_gray, curr_gray, width, height, roi) {
                roi_flows.push(flow);
            }
        }

        if roi_flows.len() < 2 {
            return 0.0;
        }

        // Compute variance across ROIs
        self.compute_flow_variance_across_rois(&roi_flows)
    }

    /// Get face ROIs based on landmarks
    fn get_face_rois(
        &self,
        landmarks: &FaceLandmarks,
        width: usize,
        height: usize,
    ) -> Vec<(usize, usize, usize, usize)> {
        let mut rois = Vec::new();
        let roi_size = 30usize;

        // Left cheek - based on jaw and nose landmarks
        if let Some(face_center) = landmarks.face_center() {
            if let Some(left_eye) = landmarks.left_eye_center() {
                let left_cheek_x = (left_eye.0 - roi_size as f32 / 2.0).max(0.0) as usize;
                let left_cheek_y = (face_center.1).max(0.0) as usize;
                if left_cheek_x + roi_size < width && left_cheek_y + roi_size < height {
                    rois.push((left_cheek_x, left_cheek_y, roi_size, roi_size));
                }
            }

            // Nose region
            if let Some(nose) = landmarks.nose_tip() {
                let nose_x = (nose.0 - roi_size as f32 / 2.0).max(0.0) as usize;
                let nose_y = (nose.1 - roi_size as f32 / 2.0).max(0.0) as usize;
                if nose_x + roi_size < width && nose_y + roi_size < height {
                    rois.push((nose_x, nose_y, roi_size, roi_size));
                }
            }

            // Right cheek
            if let Some(right_eye) = landmarks.right_eye_center() {
                let right_cheek_x = (right_eye.0 - roi_size as f32 / 2.0).max(0.0) as usize;
                let right_cheek_y = (face_center.1).max(0.0) as usize;
                if right_cheek_x + roi_size < width && right_cheek_y + roi_size < height {
                    rois.push((right_cheek_x, right_cheek_y, roi_size, roi_size));
                }
            }
        }

        rois
    }

    /// Compute optical flow in a single ROI
    fn compute_roi_flow(
        &self,
        prev: &[u8],
        curr: &[u8],
        width: usize,
        height: usize,
        roi: &(usize, usize, usize, usize),
    ) -> Option<(f32, f32)> {
        let (rx, ry, rw, rh) = *roi;

        if rx + rw > width || ry + rh > height {
            return None;
        }

        // Simple block matching to estimate flow
        let mut total_dx = 0.0f32;
        let mut total_dy = 0.0f32;
        let mut count = 0;

        let search_range = self.window_size as i32 / 2;

        // Sample points within ROI
        for by in (0..rh).step_by(self.block_size) {
            for bx in (0..rw).step_by(self.block_size) {
                let px = rx + bx;
                let py = ry + by;

                if px + self.block_size >= width || py + self.block_size >= height {
                    continue;
                }

                // Find best match in search window
                if let Some((dx, dy)) = self.find_best_match(
                    prev, curr, width, height,
                    px, py, self.block_size, search_range,
                ) {
                    total_dx += dx;
                    total_dy += dy;
                    count += 1;
                }
            }
        }

        if count > 0 {
            Some((total_dx / count as f32, total_dy / count as f32))
        } else {
            None
        }
    }

    /// Find best matching block using SAD (Sum of Absolute Differences)
    fn find_best_match(
        &self,
        prev: &[u8],
        curr: &[u8],
        width: usize,
        height: usize,
        x: usize,
        y: usize,
        block_size: usize,
        search_range: i32,
    ) -> Option<(f32, f32)> {
        let mut best_sad = u32::MAX;
        let mut best_dx = 0i32;
        let mut best_dy = 0i32;

        // Search in window around current position
        for dy in -search_range..=search_range {
            for dx in -search_range..=search_range {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;

                if nx < 0 || ny < 0 
                    || (nx as usize + block_size) >= width 
                    || (ny as usize + block_size) >= height 
                {
                    continue;
                }

                let sad = self.compute_sad(
                    prev, curr, width,
                    x, y,
                    nx as usize, ny as usize,
                    block_size,
                );

                if sad < best_sad {
                    best_sad = sad;
                    best_dx = dx;
                    best_dy = dy;
                }
            }
        }

        Some((best_dx as f32, best_dy as f32))
    }

    /// Compute Sum of Absolute Differences between two blocks
    fn compute_sad(
        &self,
        prev: &[u8],
        curr: &[u8],
        width: usize,
        px: usize,
        py: usize,
        cx: usize,
        cy: usize,
        block_size: usize,
    ) -> u32 {
        let mut sad = 0u32;

        for by in 0..block_size {
            for bx in 0..block_size {
                let prev_idx = (py + by) * width + (px + bx);
                let curr_idx = (cy + by) * width + (cx + bx);

                if prev_idx < prev.len() && curr_idx < curr.len() {
                    let diff = (prev[prev_idx] as i32 - curr[curr_idx] as i32).abs();
                    sad += diff as u32;
                }
            }
        }

        sad
    }

    /// Compute variance of flow vectors across ROIs
    fn compute_flow_variance_across_rois(&self, flows: &[(f32, f32)]) -> f32 {
        if flows.len() < 2 {
            return 0.0;
        }

        // Compute mean flow
        let mean_x: f32 = flows.iter().map(|f| f.0).sum::<f32>() / flows.len() as f32;
        let mean_y: f32 = flows.iter().map(|f| f.1).sum::<f32>() / flows.len() as f32;

        // Compute variance
        let variance: f32 = flows
            .iter()
            .map(|f| {
                let dx = f.0 - mean_x;
                let dy = f.1 - mean_y;
                dx * dx + dy * dy
            })
            .sum::<f32>()
            / flows.len() as f32;

        variance.sqrt()
    }

    /// Analyze if motion pattern is consistent with a live face
    ///
    /// Real faces show slight independent motion in different regions
    /// (breathing, micro-expressions). Flat surfaces move uniformly.
    pub fn is_live_motion(&self, variance: f32) -> bool {
        variance >= self.variance_threshold
    }

    /// Set variance threshold
    pub fn set_threshold(&mut self, threshold: f32) {
        self.variance_threshold = threshold.max(0.0);
    }
}

/// Simple gradient-based motion detection as alternative
pub struct GradientMotionDetector {
    /// Threshold for significant motion
    threshold: f32,
}

impl GradientMotionDetector {
    /// Create new gradient motion detector
    pub fn new(threshold: f32) -> Self {
        Self { threshold }
    }

    /// Detect motion between two frames
    ///
    /// Returns the amount of motion detected (0.0 = no motion)
    pub fn detect_motion(
        &self,
        prev: &[u8],
        curr: &[u8],
        width: usize,
        height: usize,
    ) -> f32 {
        if prev.len() != curr.len() || prev.len() != width * height {
            return 0.0;
        }

        let mut total_diff = 0u64;
        let mut count = 0u64;

        // Compute absolute difference
        for (p, c) in prev.iter().zip(curr.iter()) {
            let diff = (*p as i32 - *c as i32).abs() as u64;
            total_diff += diff;
            count += 1;
        }

        if count > 0 {
            total_diff as f32 / count as f32
        } else {
            0.0
        }
    }

    /// Check if motion exceeds threshold
    pub fn has_significant_motion(
        &self,
        prev: &[u8],
        curr: &[u8],
        width: usize,
        height: usize,
    ) -> bool {
        self.detect_motion(prev, curr, width, height) >= self.threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optical_flow_creation() {
        let analyzer = OpticalFlowAnalyzer::new(0.05, 10);
        assert!((analyzer.variance_threshold - 0.05).abs() < 0.001);
    }

    #[test]
    fn test_compute_sad() {
        let analyzer = OpticalFlowAnalyzer::new(0.05, 10);
        
        // Identical blocks
        let block1 = vec![100u8; 100];
        let block2 = vec![100u8; 100];
        let sad = analyzer.compute_sad(&block1, &block2, 10, 0, 0, 0, 0, 5);
        assert_eq!(sad, 0);

        // Different blocks
        let block3 = vec![150u8; 100];
        let sad = analyzer.compute_sad(&block1, &block3, 10, 0, 0, 0, 0, 5);
        assert!(sad > 0);
    }

    #[test]
    fn test_flow_variance() {
        let analyzer = OpticalFlowAnalyzer::new(0.05, 10);

        // Same flow in all ROIs (flat surface)
        let uniform_flows = vec![(1.0, 0.0), (1.0, 0.0), (1.0, 0.0)];
        let variance_uniform = analyzer.compute_flow_variance_across_rois(&uniform_flows);
        assert!(variance_uniform < 0.01);

        // Different flows (live face)
        let varied_flows = vec![(1.0, 0.5), (0.5, 1.0), (0.0, 0.2)];
        let variance_varied = analyzer.compute_flow_variance_across_rois(&varied_flows);
        assert!(variance_varied > variance_uniform);
    }

    #[test]
    fn test_is_live_motion() {
        let analyzer = OpticalFlowAnalyzer::new(0.05, 10);
        
        assert!(!analyzer.is_live_motion(0.02)); // Below threshold
        assert!(analyzer.is_live_motion(0.10));  // Above threshold
    }

    #[test]
    fn test_gradient_motion() {
        let detector = GradientMotionDetector::new(5.0);

        // No motion
        let frame1 = vec![100u8; 100];
        let frame2 = vec![100u8; 100];
        let motion = detector.detect_motion(&frame1, &frame2, 10, 10);
        assert!(motion < 0.01);

        // Some motion
        let frame3 = vec![120u8; 100];
        let motion = detector.detect_motion(&frame1, &frame3, 10, 10);
        assert!((motion - 20.0).abs() < 0.01);
    }

    #[test]
    fn test_significant_motion_threshold() {
        let detector = GradientMotionDetector::new(10.0);

        let frame1 = vec![100u8; 100];
        let frame2 = vec![105u8; 100]; // 5 difference
        assert!(!detector.has_significant_motion(&frame1, &frame2, 10, 10));

        let frame3 = vec![115u8; 100]; // 15 difference
        assert!(detector.has_significant_motion(&frame1, &frame3, 10, 10));
    }
}
