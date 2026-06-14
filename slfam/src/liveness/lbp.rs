//! Local Binary Pattern (LBP) texture analysis for liveness detection
//!
//! Analyzes skin texture to distinguish real faces from printed photos or screens.

use crate::detection::FaceLandmarks;
use crate::error::Result;

/// LBP texture analyzer
pub struct LbpTextureAnalyzer {
    /// Radius for LBP computation
    radius: usize,
    /// Number of neighbors (typically 8)
    neighbors: usize,
    /// Histogram bins
    bins: usize,
    /// Threshold for real vs fake classification
    threshold: f32,
}

impl LbpTextureAnalyzer {
    /// Create a new LBP analyzer with default settings
    pub fn new() -> Self {
        Self {
            radius: 1,
            neighbors: 8,
            bins: 256,
            threshold: 0.5,
        }
    }

    /// Create with custom parameters
    pub fn with_params(radius: usize, neighbors: usize, threshold: f32) -> Self {
        Self {
            radius,
            neighbors,
            bins: 1 << neighbors.min(8),
            threshold,
        }
    }

    /// Analyze texture of face region
    ///
    /// # Arguments
    ///
    /// * `gray` - Grayscale image data
    /// * `width` - Image width
    /// * `height` - Image height
    /// * `landmarks` - Facial landmarks for ROI extraction
    ///
    /// # Returns
    ///
    /// Tuple of (is_real, confidence_score)
    pub fn analyze(
        &self,
        gray: &[u8],
        width: usize,
        height: usize,
        landmarks: &FaceLandmarks,
    ) -> Result<(bool, f32)> {
        // Get face region for analysis
        let rois = self.get_skin_rois(landmarks, width, height);

        if rois.is_empty() {
            return Ok((false, 0.0));
        }

        // Compute LBP histogram for each ROI
        let mut all_histograms = Vec::new();

        for roi in &rois {
            let histogram = self.compute_lbp_histogram(gray, width, height, roi);
            all_histograms.push(histogram);
        }

        // Analyze histogram features
        let score = self.analyze_histograms(&all_histograms);
        let is_real = score >= self.threshold;

        Ok((is_real, score))
    }

    /// Get ROIs for skin texture analysis
    fn get_skin_rois(
        &self,
        landmarks: &FaceLandmarks,
        width: usize,
        height: usize,
    ) -> Vec<(usize, usize, usize, usize)> {
        let mut rois = Vec::new();
        let roi_size = 25usize;

        // Forehead region (above eyebrows)
        if landmarks.left_eyebrow().len() >= 3 {
            let eyebrow = landmarks.left_eyebrow();
            let center_x = eyebrow.iter().map(|p| p.0).sum::<f32>() / eyebrow.len() as f32;
            let min_y = eyebrow.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
            
            let x = (center_x - roi_size as f32 / 2.0).max(0.0) as usize;
            let y = (min_y - roi_size as f32 * 1.5).max(0.0) as usize;
            
            if x + roi_size < width && y + roi_size < height {
                rois.push((x, y, roi_size, roi_size));
            }
        }

        // Cheek regions (based on face landmarks)
        if let (Some(left_eye), Some(face_center)) = (landmarks.left_eye_center(), landmarks.face_center()) {
            let left_cheek_x = (left_eye.0 - roi_size as f32).max(0.0) as usize;
            let cheek_y = ((left_eye.1 + face_center.1) / 2.0).max(0.0) as usize;
            
            if left_cheek_x + roi_size < width && cheek_y + roi_size < height {
                rois.push((left_cheek_x, cheek_y, roi_size, roi_size));
            }
        }

        if let (Some(right_eye), Some(face_center)) = (landmarks.right_eye_center(), landmarks.face_center()) {
            let right_cheek_x = right_eye.0 as usize;
            let cheek_y = ((right_eye.1 + face_center.1) / 2.0).max(0.0) as usize;
            
            if right_cheek_x + roi_size < width && cheek_y + roi_size < height {
                rois.push((right_cheek_x, cheek_y, roi_size, roi_size));
            }
        }

        rois
    }

    /// Compute LBP histogram for a region
    fn compute_lbp_histogram(
        &self,
        gray: &[u8],
        width: usize,
        _height: usize,
        roi: &(usize, usize, usize, usize),
    ) -> Vec<f32> {
        let (rx, ry, rw, rh) = *roi;
        let mut histogram = vec![0.0f32; self.bins];
        let mut count = 0;

        // Compute LBP for each pixel in ROI (excluding border)
        for y in (ry + self.radius)..(ry + rh - self.radius) {
            for x in (rx + self.radius)..(rx + rw - self.radius) {
                let lbp = self.compute_lbp_pixel(gray, width, x, y);
                if (lbp as usize) < histogram.len() {
                    histogram[lbp as usize] += 1.0;
                    count += 1;
                }
            }
        }

        // Normalize
        if count > 0 {
            for h in &mut histogram {
                *h /= count as f32;
            }
        }

        histogram
    }

    /// Compute LBP value for a single pixel
    fn compute_lbp_pixel(&self, gray: &[u8], width: usize, x: usize, y: usize) -> u8 {
        let center = gray[y * width + x];
        let mut lbp: u8 = 0;

        // 8 neighbors in clockwise order
        let offsets: [(i32, i32); 8] = [
            (-1, -1), (0, -1), (1, -1),
            (1, 0),
            (1, 1), (0, 1), (-1, 1),
            (-1, 0),
        ];

        for (i, (dx, dy)) in offsets.iter().enumerate() {
            let nx = (x as i32 + dx) as usize;
            let ny = (y as i32 + dy) as usize;
            let neighbor = gray[ny * width + nx];

            if neighbor >= center {
                lbp |= 1 << i;
            }
        }

        lbp
    }

    /// Analyze histogram features to determine if texture is from real skin
    fn analyze_histograms(&self, histograms: &[Vec<f32>]) -> f32 {
        if histograms.is_empty() {
            return 0.0;
        }

        let mut total_score = 0.0;

        for histogram in histograms {
            // Feature 1: Uniformity - real skin has more uniform texture
            let uniformity = self.compute_uniformity(histogram);
            
            // Feature 2: Entropy - real skin has moderate entropy
            let entropy = self.compute_entropy(histogram);
            
            // Feature 3: Number of uniform patterns (8 transitions or less)
            let uniform_ratio = self.compute_uniform_pattern_ratio(histogram);

            // Combine features (weights determined empirically)
            // Real skin typically has: moderate uniformity, moderate entropy, high uniform ratio
            let score = 0.3 * uniformity + 0.3 * self.entropy_score(entropy) + 0.4 * uniform_ratio;
            total_score += score;
        }

        total_score / histograms.len() as f32
    }

    /// Compute uniformity (sum of squared probabilities)
    fn compute_uniformity(&self, histogram: &[f32]) -> f32 {
        histogram.iter().map(|p| p * p).sum::<f32>()
    }

    /// Compute entropy
    fn compute_entropy(&self, histogram: &[f32]) -> f32 {
        let mut entropy = 0.0f32;
        for &p in histogram {
            if p > 1e-10 {
                entropy -= p * p.log2();
            }
        }
        entropy
    }

    /// Convert entropy to a score (moderate entropy is best)
    fn entropy_score(&self, entropy: f32) -> f32 {
        // Optimal entropy for skin is around 4-6 bits
        let optimal = 5.0;
        let diff = (entropy - optimal).abs();
        (1.0 - diff / optimal).max(0.0)
    }

    /// Compute ratio of uniform LBP patterns
    fn compute_uniform_pattern_ratio(&self, histogram: &[f32]) -> f32 {
        // Uniform patterns are those with at most 2 bit transitions
        let uniform_patterns = self.get_uniform_patterns();
        
        let uniform_sum: f32 = uniform_patterns
            .iter()
            .filter(|&&p| (p as usize) < histogram.len())
            .map(|&p| histogram[p as usize])
            .sum();

        uniform_sum
    }

    /// Get list of uniform LBP patterns (at most 2 transitions)
    fn get_uniform_patterns(&self) -> Vec<u8> {
        let mut patterns = Vec::new();
        
        for i in 0u8..=255 {
            if self.is_uniform_pattern(i) {
                patterns.push(i);
            }
        }
        
        patterns
    }

    /// Check if LBP pattern is uniform (at most 2 bit transitions)
    fn is_uniform_pattern(&self, pattern: u8) -> bool {
        let mut transitions = 0;
        let mut prev = pattern & 1;
        
        for i in 1..8 {
            let curr = (pattern >> i) & 1;
            if curr != prev {
                transitions += 1;
            }
            prev = curr;
        }
        
        // Check wrap-around
        if ((pattern >> 7) & 1) != (pattern & 1) {
            transitions += 1;
        }
        
        transitions <= 2
    }

    /// Set classification threshold
    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold.clamp(0.0, 1.0);
    }
}

impl Default for LbpTextureAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Simplified texture analysis using gradient statistics
pub struct GradientTextureAnalyzer {
    /// Threshold for classification
    threshold: f32,
}

impl GradientTextureAnalyzer {
    /// Create new analyzer
    pub fn new(threshold: f32) -> Self {
        Self { threshold }
    }

    /// Analyze texture using gradient magnitude statistics
    pub fn analyze(&self, gray: &[u8], width: usize, height: usize) -> (bool, f32) {
        let gradients = self.compute_gradients(gray, width, height);
        
        // Compute statistics
        let mean = gradients.iter().sum::<f32>() / gradients.len() as f32;
        let variance = gradients
            .iter()
            .map(|&g| (g - mean).powi(2))
            .sum::<f32>()
            / gradients.len() as f32;
        let std_dev = variance.sqrt();

        // Real skin has moderate texture variation
        // Printed images often have lower variance or very high variance (artifacts)
        let normalized_std = std_dev / 50.0; // Normalize to typical range
        let score = (normalized_std * (1.0 - normalized_std.abs() / 2.0)).max(0.0).min(1.0);

        (score >= self.threshold, score)
    }

    /// Compute gradient magnitudes
    fn compute_gradients(&self, gray: &[u8], width: usize, height: usize) -> Vec<f32> {
        let mut gradients = Vec::with_capacity((width - 2) * (height - 2));

        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let idx = y * width + x;
                
                // Sobel-like gradient
                let gx = gray[idx + 1] as f32 - gray[idx - 1] as f32;
                let gy = gray[idx + width] as f32 - gray[idx - width] as f32;
                
                let magnitude = (gx * gx + gy * gy).sqrt();
                gradients.push(magnitude);
            }
        }

        gradients
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lbp_analyzer_creation() {
        let analyzer = LbpTextureAnalyzer::new();
        assert_eq!(analyzer.radius, 1);
        assert_eq!(analyzer.neighbors, 8);
    }

    #[test]
    fn test_uniform_pattern_detection() {
        let analyzer = LbpTextureAnalyzer::new();
        
        // 0 and 255 are uniform (all same)
        assert!(analyzer.is_uniform_pattern(0));
        assert!(analyzer.is_uniform_pattern(255));
        
        // 0b00001111 is uniform (one transition group)
        assert!(analyzer.is_uniform_pattern(0b00001111));
        
        // 0b01010101 has many transitions - not uniform
        assert!(!analyzer.is_uniform_pattern(0b01010101));
    }

    #[test]
    fn test_lbp_pixel_computation() {
        let analyzer = LbpTextureAnalyzer::new();
        
        // Simple test image: center pixel 100, all neighbors 150
        // Should get LBP = 255 (all neighbors >= center)
        let gray = vec![
            150, 150, 150, 150, 150,
            150, 150, 150, 150, 150,
            150, 150, 100, 150, 150,
            150, 150, 150, 150, 150,
            150, 150, 150, 150, 150,
        ];
        
        let lbp = analyzer.compute_lbp_pixel(&gray, 5, 2, 2);
        assert_eq!(lbp, 255);

        // Center pixel 150, all neighbors 100
        // Should get LBP = 0
        let gray2 = vec![
            100, 100, 100, 100, 100,
            100, 100, 100, 100, 100,
            100, 100, 150, 100, 100,
            100, 100, 100, 100, 100,
            100, 100, 100, 100, 100,
        ];
        
        let lbp2 = analyzer.compute_lbp_pixel(&gray2, 5, 2, 2);
        assert_eq!(lbp2, 0);
    }

    #[test]
    fn test_entropy_computation() {
        let analyzer = LbpTextureAnalyzer::new();
        
        // Uniform distribution
        let uniform: Vec<f32> = vec![1.0 / 256.0; 256];
        let entropy_uniform = analyzer.compute_entropy(&uniform);
        assert!((entropy_uniform - 8.0).abs() < 0.01); // log2(256) = 8

        // Single peak (zero entropy)
        let mut single_peak = vec![0.0f32; 256];
        single_peak[0] = 1.0;
        let entropy_single = analyzer.compute_entropy(&single_peak);
        assert!(entropy_single < 0.01);
    }

    #[test]
    fn test_uniformity_computation() {
        let analyzer = LbpTextureAnalyzer::new();
        
        // Single peak (maximum uniformity)
        let mut single_peak = vec![0.0f32; 256];
        single_peak[0] = 1.0;
        let uniformity_single = analyzer.compute_uniformity(&single_peak);
        assert!((uniformity_single - 1.0).abs() < 0.01);

        // Uniform distribution (minimum uniformity)
        let uniform: Vec<f32> = vec![1.0 / 256.0; 256];
        let uniformity_uniform = analyzer.compute_uniformity(&uniform);
        assert!(uniformity_uniform < 0.01);
    }

    #[test]
    fn test_gradient_analyzer() {
        let analyzer = GradientTextureAnalyzer::new(0.3);
        
        // Uniform image (no texture)
        let uniform = vec![128u8; 100];
        let (is_real, score) = analyzer.analyze(&uniform, 10, 10);
        assert!(score < 0.1);

        // Textured image
        let mut textured = Vec::new();
        for y in 0..10 {
            for x in 0..10 {
                textured.push(((x + y) * 20) as u8);
            }
        }
        let (_, score2) = analyzer.analyze(&textured, 10, 10);
        assert!(score2 > score);
    }
}
