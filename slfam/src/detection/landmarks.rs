//! Facial landmark detection (supports 68-point, 106-point, and 5-point models)

use super::onnx::{OnnxModel, crop_and_resize, preprocess_image_simple};
use super::BoundingBox;
use crate::camera::Frame;
use crate::error::{DetectionError, Result};
use std::path::Path;

/// Landmark format detected from the model output
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandmarkFormat {
    /// 5-point (both eye centers, nose, mouth corners)
    FivePoint,
    /// 68-point dlib convention
    SixtyEight,
    /// 106-point InsightFace convention (2d106det)
    OneSixPoint,
}

/// Facial landmarks with unified access regardless of point count
///
/// ## Index layouts:
///
/// **68-point (dlib):**
///   0-16 jaw, 17-21 left eyebrow, 22-26 right eyebrow,
///   27-35 nose, 36-41 left eye, 42-47 right eye, 48-67 mouth
///
/// **106-point (InsightFace 2d106det):**
///   0-32 face contour, 33-41 left eyebrow, 42-50 right eyebrow,
///   51-62 nose, 63-71 left eye, 72-75 eye bridge, 76-83 right eye,
///   84-86 eye bridge, 87-105 mouth
///
/// **5-point:**
///   0 left eye center, 1 right eye center, 2 nose, 3 left mouth, 4 right mouth
#[derive(Debug, Clone)]
pub struct FaceLandmarks {
    /// All landmark points (x, y)
    points: Vec<(f32, f32)>,
    /// Original image dimensions
    image_size: (u32, u32),
    /// Detected format
    format: LandmarkFormat,
}

impl FaceLandmarks {
    /// Create new landmarks from points (auto-detects format)
    pub fn new(points: Vec<(f32, f32)>, image_size: (u32, u32)) -> Self {
        let format = match points.len() {
            0..=5 => LandmarkFormat::FivePoint,
            6..=68 => LandmarkFormat::SixtyEight,
            _ => LandmarkFormat::OneSixPoint,
        };
        Self { points, image_size, format }
    }

    /// Create with explicit format
    pub fn with_format(points: Vec<(f32, f32)>, image_size: (u32, u32), format: LandmarkFormat) -> Self {
        Self { points, image_size, format }
    }

    /// Get the landmark format
    #[must_use]
    pub fn format(&self) -> LandmarkFormat {
        self.format
    }

    /// Get all landmark points
    #[must_use]
    pub fn points(&self) -> &[(f32, f32)] {
        &self.points
    }

    /// Get jaw/face contour landmarks
    #[must_use]
    pub fn jaw(&self) -> &[(f32, f32)] {
        match self.format {
            LandmarkFormat::SixtyEight if self.points.len() >= 17 => &self.points[0..17],
            LandmarkFormat::OneSixPoint if self.points.len() >= 33 => &self.points[0..33],
            _ => &[],
        }
    }

    /// Get left eyebrow landmarks
    #[must_use]
    pub fn left_eyebrow(&self) -> &[(f32, f32)] {
        match self.format {
            LandmarkFormat::SixtyEight if self.points.len() >= 22 => &self.points[17..22],
            LandmarkFormat::OneSixPoint if self.points.len() >= 42 => &self.points[33..42],
            _ => &[],
        }
    }

    /// Get right eyebrow landmarks
    #[must_use]
    pub fn right_eyebrow(&self) -> &[(f32, f32)] {
        match self.format {
            LandmarkFormat::SixtyEight if self.points.len() >= 27 => &self.points[22..27],
            LandmarkFormat::OneSixPoint if self.points.len() >= 51 => &self.points[42..51],
            _ => &[],
        }
    }

    /// Get nose landmarks
    #[must_use]
    pub fn nose(&self) -> &[(f32, f32)] {
        match self.format {
            LandmarkFormat::SixtyEight if self.points.len() >= 36 => &self.points[27..36],
            LandmarkFormat::OneSixPoint if self.points.len() >= 63 => &self.points[51..63],
            _ => &[],
        }
    }

    /// Get left eye landmarks (6 points for EAR calculation)
    ///
    /// For 106-point, maps the 9 eye points to 6-point EAR format:
    /// [left_corner, top_left, top_right, right_corner, bottom_right, bottom_left]
    #[must_use]
    pub fn left_eye(&self) -> &[(f32, f32)] {
        match self.format {
            LandmarkFormat::SixtyEight if self.points.len() >= 42 => &self.points[36..42],
            LandmarkFormat::OneSixPoint if self.points.len() >= 72 => &self.points[63..72],
            _ => &[],
        }
    }

    /// Get right eye landmarks (6 points for EAR calculation)
    ///
    /// For 106-point, maps the 8 eye points:
    #[must_use]
    pub fn right_eye(&self) -> &[(f32, f32)] {
        match self.format {
            LandmarkFormat::SixtyEight if self.points.len() >= 48 => &self.points[42..48],
            LandmarkFormat::OneSixPoint if self.points.len() >= 84 => &self.points[76..84],
            _ => &[],
        }
    }

    /// Get left eye as exactly 6 points for EAR (Eye Aspect Ratio) computation.
    /// Maps any format to: [left_corner, top_left, top_right, right_corner, bottom_right, bottom_left]
    #[must_use]
    pub fn left_eye_ear6(&self) -> Option<[(f32, f32); 6]> {
        match self.format {
            LandmarkFormat::SixtyEight if self.points.len() >= 42 => {
                // 68-point: indices 36-41 are already in EAR order
                Some([
                    self.points[36], self.points[37], self.points[38],
                    self.points[39], self.points[40], self.points[41],
                ])
            }
            LandmarkFormat::OneSixPoint if self.points.len() >= 72 => {
                // 106-point left eye: 63-71 (9 points)
                // Map to 6-point: corner(63), top-left(64), top-right(65),
                //                  corner(66), bottom-right(68), bottom-left(70)
                Some([
                    self.points[63], self.points[64], self.points[65],
                    self.points[66], self.points[68], self.points[70],
                ])
            }
            LandmarkFormat::FivePoint if self.points.len() >= 2 => None, // not enough for EAR
            _ => None,
        }
    }

    /// Get right eye as exactly 6 points for EAR computation.
    #[must_use]
    pub fn right_eye_ear6(&self) -> Option<[(f32, f32); 6]> {
        match self.format {
            LandmarkFormat::SixtyEight if self.points.len() >= 48 => {
                Some([
                    self.points[42], self.points[43], self.points[44],
                    self.points[45], self.points[46], self.points[47],
                ])
            }
            LandmarkFormat::OneSixPoint if self.points.len() >= 84 => {
                // 106-point right eye: 76-83 (8 points)
                // Map to 6-point: corner(76), top-left(77), top-right(78),
                //                  corner(79), bottom-right(81), bottom-left(83)
                Some([
                    self.points[76], self.points[77], self.points[78],
                    self.points[79], self.points[81], self.points[83],
                ])
            }
            _ => None,
        }
    }

    /// Get mouth landmarks
    #[must_use]
    pub fn mouth(&self) -> &[(f32, f32)] {
        match self.format {
            LandmarkFormat::SixtyEight if self.points.len() >= 68 => &self.points[48..68],
            LandmarkFormat::OneSixPoint if self.points.len() >= 106 => &self.points[87..106],
            _ => &[],
        }
    }

    /// Get outer mouth landmarks
    #[must_use]
    pub fn outer_mouth(&self) -> &[(f32, f32)] {
        match self.format {
            LandmarkFormat::SixtyEight if self.points.len() >= 60 => &self.points[48..60],
            LandmarkFormat::OneSixPoint if self.points.len() >= 100 => &self.points[87..100],
            _ => &[],
        }
    }

    /// Get inner mouth landmarks
    #[must_use]
    pub fn inner_mouth(&self) -> &[(f32, f32)] {
        match self.format {
            LandmarkFormat::SixtyEight if self.points.len() >= 68 => &self.points[60..68],
            LandmarkFormat::OneSixPoint if self.points.len() >= 106 => &self.points[100..106],
            _ => &[],
        }
    }

    /// Get left eye center
    #[must_use]
    pub fn left_eye_center(&self) -> Option<(f32, f32)> {
        match self.format {
            LandmarkFormat::FivePoint if !self.points.is_empty() => Some(self.points[0]),
            _ => {
                let eye = self.left_eye();
                if eye.is_empty() {
                    return None;
                }
                let sum: (f32, f32) = eye.iter().fold((0.0, 0.0), |acc, p| (acc.0 + p.0, acc.1 + p.1));
                Some((sum.0 / eye.len() as f32, sum.1 / eye.len() as f32))
            }
        }
    }

    /// Get right eye center
    #[must_use]
    pub fn right_eye_center(&self) -> Option<(f32, f32)> {
        match self.format {
            LandmarkFormat::FivePoint if self.points.len() >= 2 => Some(self.points[1]),
            _ => {
                let eye = self.right_eye();
                if eye.is_empty() {
                    return None;
                }
                let sum: (f32, f32) = eye.iter().fold((0.0, 0.0), |acc, p| (acc.0 + p.0, acc.1 + p.1));
                Some((sum.0 / eye.len() as f32, sum.1 / eye.len() as f32))
            }
        }
    }

    /// Get nose tip
    #[must_use]
    pub fn nose_tip(&self) -> Option<(f32, f32)> {
        match self.format {
            LandmarkFormat::FivePoint if self.points.len() >= 3 => Some(self.points[2]),
            LandmarkFormat::SixtyEight if self.points.len() > 30 => Some(self.points[30]),
            LandmarkFormat::OneSixPoint if self.points.len() > 51 => Some(self.points[51]),
            _ => None,
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
        match self.format {
            LandmarkFormat::FivePoint if self.points.len() >= 5 => {
                Some([
                    self.points[0], self.points[1], self.points[2],
                    self.points[3], self.points[4],
                ])
            }
            LandmarkFormat::SixtyEight if self.points.len() >= 68 => {
                let left_eye = self.left_eye_center()?;
                let right_eye = self.right_eye_center()?;
                let nose = self.nose_tip()?;
                let left_mouth = self.points[48];
                let right_mouth = self.points[54];
                Some([left_eye, right_eye, nose, left_mouth, right_mouth])
            }
            LandmarkFormat::OneSixPoint if self.points.len() >= 106 => {
                let left_eye = self.left_eye_center()?;
                let right_eye = self.right_eye_center()?;
                let nose = self.points[51]; // nose tip
                let left_mouth = self.points[87]; // left mouth corner
                let right_mouth = self.points[93]; // right mouth corner
                Some([left_eye, right_eye, nose, left_mouth, right_mouth])
            }
            _ => None,
        }
    }

    /// Check if landmarks appear valid (basic sanity checks)
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let min_points = match self.format {
            LandmarkFormat::FivePoint => 5,
            LandmarkFormat::SixtyEight => 68,
            LandmarkFormat::OneSixPoint => 106,
        };

        if self.points.len() < min_points {
            return false;
        }

        // Check all points are within image bounds (with some tolerance)
        let tol = 10.0;
        for (x, y) in &self.points {
            if *x < -tol || *y < -tol
                || *x > self.image_size.0 as f32 + tol
                || *y > self.image_size.1 as f32 + tol
            {
                return false;
            }
        }

        // Check eyes are above nose (basic geometry sanity)
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

/// Landmark detector using ONNX model (supports 68 or 106 point output)
pub struct LandmarkDetector {
    /// ONNX model
    model: OnnxModel,
    /// Input size for the model
    input_size: (usize, usize),
}

impl LandmarkDetector {
    /// Load a landmark detection model
    pub fn load<P: AsRef<Path>>(model_path: P) -> Result<Self> {
        let model = OnnxModel::load(model_path)?;
        let input_size = (model.input_width(), model.input_height());
        Ok(Self { model, input_size })
    }

    /// Detect landmarks in a face region
    pub fn detect(&self, frame: &Frame, face_bbox: &BoundingBox) -> Result<FaceLandmarks> {
        let frame_bgr = frame.to_bgr24()?;
        let data = frame_bgr.data();
        let width = frame.width() as usize;
        let height = frame.height() as usize;

        let expanded = face_bbox.expand(1.2);
        let (x, y, w, h) = expanded.to_rect(frame.width(), frame.height());

        let face_data = crop_and_resize(
            data, width, height,
            x as usize, y as usize, w as usize, h as usize,
            self.input_size.0, self.input_size.1, 3,
        );

        let input = preprocess_image_simple(
            &face_data,
            self.input_size.0, self.input_size.1,
            self.input_size.0, self.input_size.1,
        );

        let outputs = self.model.run(input)?;
        let landmarks = self.parse_output(&outputs, x as f32, y as f32, w as f32, h as f32)?;

        let format = match landmarks.len() {
            5 => LandmarkFormat::FivePoint,
            68 => LandmarkFormat::SixtyEight,
            106 => LandmarkFormat::OneSixPoint,
            n if n >= 106 => LandmarkFormat::OneSixPoint,
            n if n >= 68 => LandmarkFormat::SixtyEight,
            _ => LandmarkFormat::FivePoint,
        };

        Ok(FaceLandmarks::with_format(
            landmarks,
            (frame.width(), frame.height()),
            format,
        ))
    }

    /// Parse model output to landmark points.
    /// Handles both 136-value (68pt) and 212-value (106pt) outputs.
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

        // Determine number of points from output size
        let num_values = flat.len();
        let num_points = if num_values >= 212 {
            106 // InsightFace 2d106det
        } else if num_values >= 136 {
            68 // dlib-style
        } else if num_values >= 10 {
            num_values / 2
        } else {
            return Err(DetectionError::LandmarkFailed(format!(
                "Output too small: {} values (need at least 10 for 5 points)",
                num_values
            )).into());
        };

        // Convert normalized coordinates to image coordinates
        let mut points = Vec::with_capacity(num_points);
        for i in 0..num_points {
            let x_norm = flat[i * 2];
            let y_norm = flat[i * 2 + 1];

            let x = bbox_x + x_norm * bbox_w;
            let y = bbox_y + y_norm * bbox_h;
            points.push((x, y));
        }

        Ok(points)
    }
}

/// Simple landmark estimator based on face proportions (fallback when no model available)
pub struct SimpleLandmarkEstimator;

impl SimpleLandmarkEstimator {
    /// Estimate 68-point landmarks based on average face proportions
    pub fn estimate(bbox: &BoundingBox, image_size: (u32, u32)) -> FaceLandmarks {
        let cx = bbox.x + bbox.width / 2.0;
        let cy = bbox.y + bbox.height / 2.0;
        let w = bbox.width;
        let h = bbox.height;

        let mut points = Vec::with_capacity(68);

        // Jaw line (0-16)
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
            let x = if i < 4 { cx } else { cx + (i as f32 - 6.0) * w * 0.05 };
            points.push((x, y));
        }

        // Left eye (36-41) - 6 points for EAR
        let left_eye_x = cx - w * 0.2;
        let left_eye_y = cy - h * 0.1;
        let eye_w = w * 0.15;
        let eye_h = h * 0.08;
        points.push((left_eye_x - eye_w / 2.0, left_eye_y));
        points.push((left_eye_x - eye_w / 4.0, left_eye_y - eye_h / 2.0));
        points.push((left_eye_x + eye_w / 4.0, left_eye_y - eye_h / 2.0));
        points.push((left_eye_x + eye_w / 2.0, left_eye_y));
        points.push((left_eye_x + eye_w / 4.0, left_eye_y + eye_h / 2.0));
        points.push((left_eye_x - eye_w / 4.0, left_eye_y + eye_h / 2.0));

        // Right eye (42-47)
        let right_eye_x = cx + w * 0.2;
        let right_eye_y = cy - h * 0.1;
        points.push((right_eye_x - eye_w / 2.0, right_eye_y));
        points.push((right_eye_x - eye_w / 4.0, right_eye_y - eye_h / 2.0));
        points.push((right_eye_x + eye_w / 4.0, right_eye_y - eye_h / 2.0));
        points.push((right_eye_x + eye_w / 2.0, right_eye_y));
        points.push((right_eye_x + eye_w / 4.0, right_eye_y + eye_h / 2.0));
        points.push((right_eye_x - eye_w / 4.0, right_eye_y + eye_h / 2.0));

        // Mouth (48-67)
        let mouth_y = cy + h * 0.3;
        let mouth_w = w * 0.35;
        let mouth_h = h * 0.12;
        for i in 0..12 {
            let angle = 2.0 * std::f32::consts::PI * (i as f32 / 12.0);
            let x = cx + (mouth_w / 2.0) * angle.cos();
            let y = mouth_y + (mouth_h / 2.0) * angle.sin();
            points.push((x, y));
        }
        for i in 0..8 {
            let angle = 2.0 * std::f32::consts::PI * (i as f32 / 8.0);
            let x = cx + (mouth_w / 4.0) * angle.cos();
            let y = mouth_y + (mouth_h / 4.0) * angle.sin();
            points.push((x, y));
        }

        FaceLandmarks::with_format(points, image_size, LandmarkFormat::SixtyEight)
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

        assert_eq!(landmarks.format(), LandmarkFormat::SixtyEight);
        assert_eq!(landmarks.left_eye().len(), 6);
        assert_eq!(landmarks.right_eye().len(), 6);
        assert_eq!(landmarks.left_eye()[0], (36.0, 36.0));
    }

    #[test]
    fn test_106_point_format_detection() {
        let points: Vec<(f32, f32)> = (0..106).map(|i| (i as f32, i as f32)).collect();
        let landmarks = FaceLandmarks::new(points, (640, 480));

        assert_eq!(landmarks.format(), LandmarkFormat::OneSixPoint);
        assert_eq!(landmarks.left_eye().len(), 9); // 63..72
        assert_eq!(landmarks.right_eye().len(), 8); // 76..84
        assert_eq!(landmarks.mouth().len(), 19); // 87..106
        assert_eq!(landmarks.nose().len(), 12); // 51..63
    }

    #[test]
    fn test_106_point_five_point_extraction() {
        let points: Vec<(f32, f32)> = (0..106).map(|i| (i as f32 * 2.0, i as f32)).collect();
        let landmarks = FaceLandmarks::new(points, (640, 480));

        let five = landmarks.five_point().unwrap();
        assert_eq!(five.len(), 5);
        // nose tip should be point 51
        assert_eq!(five[2], (51.0 * 2.0, 51.0));
        // left mouth corner should be point 87
        assert_eq!(five[3], (87.0 * 2.0, 87.0));
        // right mouth corner should be point 93
        assert_eq!(five[4], (93.0 * 2.0, 93.0));
    }

    #[test]
    fn test_106_point_ear6() {
        let points: Vec<(f32, f32)> = (0..106).map(|i| (i as f32, i as f32)).collect();
        let landmarks = FaceLandmarks::new(points, (640, 480));

        let left_ear6 = landmarks.left_eye_ear6().unwrap();
        assert_eq!(left_ear6.len(), 6);
        assert_eq!(left_ear6[0], (63.0, 63.0)); // corner
        assert_eq!(left_ear6[3], (66.0, 66.0)); // opposite corner

        let right_ear6 = landmarks.right_eye_ear6().unwrap();
        assert_eq!(right_ear6.len(), 6);
        assert_eq!(right_ear6[0], (76.0, 76.0)); // corner
        assert_eq!(right_ear6[3], (79.0, 79.0)); // opposite corner
    }

    #[test]
    fn test_landmarks_centers() {
        let mut points: Vec<(f32, f32)> = vec![(0.0, 0.0); 68];
        for i in 36..42 { points[i] = (100.0, 100.0); }
        for i in 42..48 { points[i] = (200.0, 100.0); }

        let landmarks = FaceLandmarks::new(points, (640, 480));
        let left = landmarks.left_eye_center().unwrap();
        let right = landmarks.right_eye_center().unwrap();

        assert!((left.0 - 100.0).abs() < 0.001);
        assert!((right.0 - 200.0).abs() < 0.001);
    }

    #[test]
    fn test_inter_eye_distance() {
        let mut points: Vec<(f32, f32)> = vec![(0.0, 0.0); 68];
        for i in 36..42 { points[i] = (0.0, 0.0); }
        for i in 42..48 { points[i] = (100.0, 0.0); }

        let landmarks = FaceLandmarks::new(points, (640, 480));
        let dist = landmarks.inter_eye_distance().unwrap();
        assert!((dist - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_rotation_angle() {
        let mut points: Vec<(f32, f32)> = vec![(0.0, 0.0); 68];
        for i in 36..42 { points[i] = (0.0, 0.0); }
        for i in 42..48 { points[i] = (100.0, 0.0); }

        let landmarks = FaceLandmarks::new(points, (640, 480));
        let angle = landmarks.rotation_angle().unwrap();
        assert!(angle.abs() < 0.001);
    }

    #[test]
    fn test_simple_estimator() {
        let bbox = BoundingBox::new(100.0, 100.0, 200.0, 250.0);
        let landmarks = SimpleLandmarkEstimator::estimate(&bbox, (640, 480));

        assert_eq!(landmarks.points().len(), 68);
        assert_eq!(landmarks.format(), LandmarkFormat::SixtyEight);
        assert!(landmarks.is_valid());
    }

    #[test]
    fn test_five_point_landmarks() {
        let bbox = BoundingBox::new(100.0, 100.0, 200.0, 250.0);
        let landmarks = SimpleLandmarkEstimator::estimate(&bbox, (640, 480));

        let five = landmarks.five_point();
        assert!(five.is_some());
        assert_eq!(five.unwrap().len(), 5);
    }

    #[test]
    fn test_five_point_format() {
        let points = vec![(10.0, 10.0), (50.0, 10.0), (30.0, 30.0), (15.0, 50.0), (45.0, 50.0)];
        let landmarks = FaceLandmarks::new(points, (640, 480));

        assert_eq!(landmarks.format(), LandmarkFormat::FivePoint);
        assert_eq!(landmarks.left_eye_center(), Some((10.0, 10.0)));
        assert_eq!(landmarks.right_eye_center(), Some((50.0, 10.0)));
        assert_eq!(landmarks.nose_tip(), Some((30.0, 30.0)));
    }
}
