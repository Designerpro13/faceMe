//! Facial landmark detection (68-point model)

use super::onnx::{OnnxModel, crop_and_resize, preprocess_image_simple};
use super::BoundingBox;
use crate::camera::Frame;
use crate::error::{DetectionError, Result};
use ndarray::Array4;
use std::path::Path;

/// 68-point facial landmarks following dlib convention
///
/// Points 0-16: Jaw line
/// Points 17-21: Left eyebrow
/// Points 22-26: Right eyebrow  
/// Points 27-35: Nose
/// Points 36-41: Left eye
/// Points 42-47: Right eye
/// Points 48-67: Mouth
#[derive(Debug, Clone)]
pub struct FaceLandmarks {
    /// All 68 landmark points (x, y)
    points: Vec<(f32, f32)>,
    /// Original image dimensions
    image_size: (u32, u32),
}

impl FaceLandmarks {
    /// Create new landmarks from points
    pub fn new(points: Vec<(f32, f32)>, image_size: (u32, u32)) -> Self {
        Self { points, image_size }
    }

    /// Get all landmark points
    #[must_use]
    pub fn points(&self) -> &[(f32, f32)] {
        &self.points
    }

    /// Get jaw line landmarks (0-16)
    #[must_use]
    pub fn jaw(&self) -> &[(f32, f32)] {
        if self.points.len() >= 17 {
            &self.points[0..17]
        } else {
            &[]
        }
    }

    /// Get left eyebrow landmarks (17-21)
    #[must_use]
    pub fn left_eyebrow(&self) -> &[(f32, f32)] {
        if self.points.len() >= 22 {
            &self.points[17..22]
        } else {
            &[]
        }
    }

    /// Get right eyebrow landmarks (22-26)
    #[must_use]
    pub fn right_eyebrow(&self) -> &[(f32, f32)] {
        if self.points.len() >= 27 {
            &self.points[22..27]
        } else {
            &[]
        }
    }

    /// Get nose landmarks (27-35)
    #[must_use]
    pub fn nose(&self) -> &[(f32, f32)] {
        if self.points.len() >= 36 {
            &self.points[27..36]
        } else {
            &[]
        }
    }

    /// Get left eye landmarks (36-41) - 6 points for EAR calculation
    #[must_use]
    pub fn left_eye(&self) -> &[(f32, f32)] {
        if self.points.len() >= 42 {
            &self.points[36..42]
        } else {
            &[]
        }
    }

    /// Get right eye landmarks (42-47) - 6 points for EAR calculation
    #[must_use]
    pub fn right_eye(&self) -> &[(f32, f32)] {
        if self.points.len() >= 48 {
            &self.points[42..48]
        } else {
            &[]
        }
    }

    /// Get mouth landmarks (48-67)
    #[must_use]
    pub fn mouth(&self) -> &[(f32, f32)] {
        if self.points.len() >= 68 {
            &self.points[48..68]
        } else {
            &[]
        }
    }

    /// Get outer mouth landmarks (48-59)
    #[must_use]
    pub fn outer_mouth(&self) -> &[(f32, f32)] {
        if self.points.len() >= 60 {
            &self.points[48..60]
        } else {
            &[]
        }
    }

    /// Get inner mouth landmarks (60-67)
    #[must_use]
    pub fn inner_mouth(&self) -> &[(f32, f32)] {
        if self.points.len() >= 68 {
            &self.points[60..68]
        } else {
            &[]
        }
    }

    /// Get left eye center
    #[must_use]
    pub fn left_eye_center(&self) -> Option<(f32, f32)> {
        let eye = self.left_eye();
        if eye.is_empty() {
            return None;
        }
        let sum: (f32, f32) = eye.iter().fold((0.0, 0.0), |acc, p| (acc.0 + p.0, acc.1 + p.1));
        Some((sum.0 / eye.len() as f32, sum.1 / eye.len() as f32))
    }

    /// Get right eye center
    #[must_use]
    pub fn right_eye_center(&self) -> Option<(f32, f32)> {
        let eye = self.right_eye();
        if eye.is_empty() {
            return None;
        }
        let sum: (f32, f32) = eye.iter().fold((0.0, 0.0), |acc, p| (acc.0 + p.0, acc.1 + p.1));
        Some((sum.0 / eye.len() as f32, sum.1 / eye.len() as f32))
    }

    /// Get nose tip (point 30)
    #[must_use]
    pub fn nose_tip(&self) -> Option<(f32, f32)> {
        if self.points.len() > 30 {
            Some(self.points[30])
        } else {
            None
        }
    }

    /// Get face center (average of eye centers and nose tip)
    #[must_use]
    pub fn face_center(&self) -> Option<(f32, f32)> {
        let left = self.left_eye_center()?;
        let right = self.right_eye_center()?;
        let nose = self.nose_tip()?;
        Some((
            (left.0 + right.0 + nose.0) / 3.0,
            (left.1 + right.1 + nose.1) / 3.0,
        ))
    }

    /// Compute inter-eye distance
    #[must_use]
    pub fn inter_eye_distance(&self) -> Option<f32> {
        let left = self.left_eye_center()?;
        let right = self.right_eye_center()?;
        Some(euclidean_distance(left, right))
    }

    /// Compute face rotation angle (in degrees)
    #[must_use]
    pub fn rotation_angle(&self) -> Option<f32> {
        let left = self.left_eye_center()?;
        let right = self.right_eye_center()?;
        let dx = right.0 - left.0;
        let dy = right.1 - left.1;
        Some(dy.atan2(dx).to_degrees())
    }

    /// Get 5-point landmarks for alignment (both eyes, nose, mouth corners)
    #[must_use]
    pub fn five_point(&self) -> Option<[(f32, f32); 5]> {
        let left_eye = self.left_eye_center()?;
        let right_eye = self.right_eye_center()?;
        let nose = self.nose_tip()?;
        
        if self.points.len() < 68 {
            return None;
        }
        
        let left_mouth = self.points[48];
        let right_mouth = self.points[54];
        
        Some([left_eye, right_eye, nose, left_mouth, right_mouth])
    }

    /// Check if landmarks appear valid (basic sanity checks)
    #[must_use]
    pub fn is_valid(&self) -> bool {
        if self.points.len() < 68 {
            return false;
        }

        // Check all points are within image bounds
        for (x, y) in &self.points {
            if *x < 0.0 || *y < 0.0 
                || *x > self.image_size.0 as f32 
                || *y > self.image_size.1 as f32 
            {
                return false;
            }
        }

        // Check eyes are above mouth
        if let (Some(left_eye), Some(nose)) = (self.left_eye_center(), self.nose_tip()) {
            if left_eye.1 > nose.1 + 20.0 {
                return false;
            }
        }

        true
    }
}

/// Compute Euclidean distance between two points
#[inline]
pub fn euclidean_distance(p1: (f32, f32), p2: (f32, f32)) -> f32 {
    let dx = p2.0 - p1.0;
    let dy = p2.1 - p1.1;
    (dx * dx + dy * dy).sqrt()
}

/// 68-point landmark detector using ONNX model
pub struct LandmarkDetector {
    /// ONNX model
    model: OnnxModel,
    /// Input size for the model
    input_size: (usize, usize),
}

impl LandmarkDetector {
    /// Load a landmark detection model
    ///
    /// # Arguments
    ///
    /// * `model_path` - Path to the ONNX model
    ///
    /// # Errors
    ///
    /// Returns an error if the model cannot be loaded
    pub fn load<P: AsRef<Path>>(model_path: P) -> Result<Self> {
        let model = OnnxModel::load(model_path)?;
        let input_size = (model.input_width(), model.input_height());

        Ok(Self { model, input_size })
    }

    /// Detect landmarks in a face region
    ///
    /// # Arguments
    ///
    /// * `frame` - Input frame
    /// * `face_bbox` - Bounding box of the detected face
    ///
    /// # Errors
    ///
    /// Returns an error if detection fails
    pub fn detect(&self, frame: &Frame, face_bbox: &BoundingBox) -> Result<FaceLandmarks> {
        let frame_bgr = frame.to_bgr24()?;
        let data = frame_bgr.data();
        let width = frame.width() as usize;
        let height = frame.height() as usize;

        // Expand face bbox slightly for better landmark detection
        let expanded = face_bbox.expand(1.2);
        let (x, y, w, h) = expanded.to_rect(frame.width(), frame.height());

        // Crop and resize face region
        let face_data = crop_and_resize(
            data,
            width,
            height,
            x as usize,
            y as usize,
            w as usize,
            h as usize,
            self.input_size.0,
            self.input_size.1,
            3,
        );

        // Preprocess for model
        let input = preprocess_image_simple(
            &face_data,
            self.input_size.0,
            self.input_size.1,
            self.input_size.0,
            self.input_size.1,
        );

        // Run inference
        let outputs = self.model.run(input)?;

        // Parse output (expected shape: [1, 136] for 68 points * 2 coordinates)
        let landmarks = self.parse_output(&outputs, x as f32, y as f32, w as f32, h as f32)?;

        Ok(FaceLandmarks::new(
            landmarks,
            (frame.width(), frame.height()),
        ))
    }

    /// Parse model output to landmark points
    fn parse_output(
        &self,
        outputs: &[ndarray::Array<f32, ndarray::IxDyn>],
        bbox_x: f32,
        bbox_y: f32,
        bbox_w: f32,
        bbox_h: f32,
    ) -> Result<Vec<(f32, f32)>> {
        if outputs.is_empty() {
            return Err(DetectionError::LandmarkFailed("No output from model".to_string()).into());
        }

        let output = &outputs[0];
        let flat = output.as_slice().ok_or_else(|| {
            DetectionError::LandmarkFailed("Cannot read output tensor".to_string())
        })?;

        if flat.len() < 136 {
            return Err(DetectionError::LandmarkFailed(format!(
                "Expected 136 values, got {}",
                flat.len()
            ))
            .into());
        }

        // Convert normalized coordinates to image coordinates
        let mut points = Vec::with_capacity(68);
        for i in 0..68 {
            let x_norm = flat[i * 2];
            let y_norm = flat[i * 2 + 1];

            // Scale to face bbox coordinates, then to image coordinates
            let x = bbox_x + x_norm * bbox_w;
            let y = bbox_y + y_norm * bbox_h;

            points.push((x, y));
        }

        Ok(points)
    }
}

/// Simple landmark estimator based on face proportions (fallback)
pub struct SimpleLandmarkEstimator;

impl SimpleLandmarkEstimator {
    /// Estimate landmarks based on average face proportions
    ///
    /// This is a fallback when no ONNX model is available
    pub fn estimate(bbox: &BoundingBox, image_size: (u32, u32)) -> FaceLandmarks {
        let cx = bbox.x + bbox.width / 2.0;
        let cy = bbox.y + bbox.height / 2.0;
        let w = bbox.width;
        let h = bbox.height;

        let mut points = Vec::with_capacity(68);

        // Generate approximate landmarks based on average face proportions
        // This is very rough but useful for testing

        // Jaw line (0-16) - arc around lower face
        for i in 0..17 {
            let angle = std::f32::consts::PI * (i as f32 / 16.0);
            let x = cx - (w * 0.45) * angle.cos();
            let y = cy + (h * 0.4) * angle.sin();
            points.push((x, y));
        }

        // Left eyebrow (17-21)
        let eyebrow_y = cy - h * 0.25;
        for i in 0..5 {
            let x = cx - w * 0.3 + (i as f32 * w * 0.12);
            points.push((x, eyebrow_y));
        }

        // Right eyebrow (22-26)
        for i in 0..5 {
            let x = cx + w * 0.05 + (i as f32 * w * 0.12);
            points.push((x, eyebrow_y));
        }

        // Nose (27-35)
        let nose_top = cy - h * 0.15;
        let nose_bottom = cy + h * 0.15;
        for i in 0..9 {
            let y = nose_top + (i as f32 / 8.0) * (nose_bottom - nose_top);
            let x = if i < 4 {
                cx
            } else {
                cx + (i as f32 - 6.0) * w * 0.05
            };
            points.push((x, y));
        }

        // Left eye (36-41) - 6 points in specific order for EAR
        let left_eye_x = cx - w * 0.2;
        let left_eye_y = cy - h * 0.1;
        let eye_w = w * 0.15;
        let eye_h = h * 0.08;
        points.push((left_eye_x - eye_w / 2.0, left_eye_y)); // 36: left corner
        points.push((left_eye_x - eye_w / 4.0, left_eye_y - eye_h / 2.0)); // 37: top-left
        points.push((left_eye_x + eye_w / 4.0, left_eye_y - eye_h / 2.0)); // 38: top-right
        points.push((left_eye_x + eye_w / 2.0, left_eye_y)); // 39: right corner
        points.push((left_eye_x + eye_w / 4.0, left_eye_y + eye_h / 2.0)); // 40: bottom-right
        points.push((left_eye_x - eye_w / 4.0, left_eye_y + eye_h / 2.0)); // 41: bottom-left

        // Right eye (42-47)
        let right_eye_x = cx + w * 0.2;
        let right_eye_y = cy - h * 0.1;
        points.push((right_eye_x - eye_w / 2.0, right_eye_y)); // 42
        points.push((right_eye_x - eye_w / 4.0, right_eye_y - eye_h / 2.0)); // 43
        points.push((right_eye_x + eye_w / 4.0, right_eye_y - eye_h / 2.0)); // 44
        points.push((right_eye_x + eye_w / 2.0, right_eye_y)); // 45
        points.push((right_eye_x + eye_w / 4.0, right_eye_y + eye_h / 2.0)); // 46
        points.push((right_eye_x - eye_w / 4.0, right_eye_y + eye_h / 2.0)); // 47

        // Mouth (48-67) - outer and inner
        let mouth_y = cy + h * 0.3;
        let mouth_w = w * 0.35;
        let mouth_h = h * 0.12;

        // Outer mouth (48-59)
        for i in 0..12 {
            let angle = 2.0 * std::f32::consts::PI * (i as f32 / 12.0);
            let x = cx + (mouth_w / 2.0) * angle.cos();
            let y = mouth_y + (mouth_h / 2.0) * angle.sin();
            points.push((x, y));
        }

        // Inner mouth (60-67)
        for i in 0..8 {
            let angle = 2.0 * std::f32::consts::PI * (i as f32 / 8.0);
            let x = cx + (mouth_w / 4.0) * angle.cos();
            let y = mouth_y + (mouth_h / 4.0) * angle.sin();
            points.push((x, y));
        }

        FaceLandmarks::new(points, image_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_euclidean_distance() {
        let d = euclidean_distance((0.0, 0.0), (3.0, 4.0));
        assert!((d - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_landmarks_eye_access() {
        let points: Vec<(f32, f32)> = (0..68).map(|i| (i as f32, i as f32)).collect();
        let landmarks = FaceLandmarks::new(points, (640, 480));

        assert_eq!(landmarks.left_eye().len(), 6);
        assert_eq!(landmarks.right_eye().len(), 6);
        assert_eq!(landmarks.left_eye()[0], (36.0, 36.0));
    }

    #[test]
    fn test_landmarks_centers() {
        let mut points: Vec<(f32, f32)> = vec![(0.0, 0.0); 68];
        
        // Set left eye points (36-41)
        for i in 36..42 {
            points[i] = (100.0, 100.0);
        }
        
        // Set right eye points (42-47)
        for i in 42..48 {
            points[i] = (200.0, 100.0);
        }

        let landmarks = FaceLandmarks::new(points, (640, 480));

        let left = landmarks.left_eye_center().unwrap();
        let right = landmarks.right_eye_center().unwrap();

        assert!((left.0 - 100.0).abs() < 0.001);
        assert!((right.0 - 200.0).abs() < 0.001);
    }

    #[test]
    fn test_inter_eye_distance() {
        let mut points: Vec<(f32, f32)> = vec![(0.0, 0.0); 68];
        
        for i in 36..42 {
            points[i] = (0.0, 0.0);
        }
        for i in 42..48 {
            points[i] = (100.0, 0.0);
        }

        let landmarks = FaceLandmarks::new(points, (640, 480));
        let dist = landmarks.inter_eye_distance().unwrap();

        assert!((dist - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_rotation_angle() {
        let mut points: Vec<(f32, f32)> = vec![(0.0, 0.0); 68];
        
        // Horizontal eyes = 0 degrees
        for i in 36..42 {
            points[i] = (0.0, 0.0);
        }
        for i in 42..48 {
            points[i] = (100.0, 0.0);
        }

        let landmarks = FaceLandmarks::new(points, (640, 480));
        let angle = landmarks.rotation_angle().unwrap();

        assert!(angle.abs() < 0.001);
    }

    #[test]
    fn test_simple_estimator() {
        let bbox = BoundingBox::new(100.0, 100.0, 200.0, 250.0);
        let landmarks = SimpleLandmarkEstimator::estimate(&bbox, (640, 480));

        assert_eq!(landmarks.points().len(), 68);
        assert!(landmarks.is_valid());
    }

    #[test]
    fn test_five_point_landmarks() {
        let bbox = BoundingBox::new(100.0, 100.0, 200.0, 250.0);
        let landmarks = SimpleLandmarkEstimator::estimate(&bbox, (640, 480));

        let five = landmarks.five_point();
        assert!(five.is_some());
        
        let five = five.unwrap();
        assert_eq!(five.len(), 5);
    }
}
