//! # Face Detection Module
//!
//! This module provides face detection, landmark extraction, and face alignment
//! for the SLFAM authentication system.
//!
//! ## Features
//!
//! - ONNX-based face detection (RetinaFace)
//! - 68-point facial landmark detection
//! - Face alignment and normalization
//! - Multi-face rejection
//! - Confidence thresholding

mod alignment;
pub mod landmarks;
pub mod onnx;
mod retinaface;

pub use alignment::{align_face, AlignedFace};
pub use landmarks::{euclidean_distance, FaceLandmarks, LandmarkDetector};
pub use onnx::{OnnxModel, preprocess_image_arcface, bilinear_resize};
pub use retinaface::{DetectedFace, FaceDetector};

use crate::camera::Frame;
use crate::config::DetectionConfig;
use crate::error::{DetectionError, Result};
use std::path::Path;

/// Bounding box for a detected face
#[derive(Debug, Clone, Copy)]
pub struct BoundingBox {
    /// Left edge x coordinate
    pub x: f32,
    /// Top edge y coordinate
    pub y: f32,
    /// Width
    pub width: f32,
    /// Height
    pub height: f32,
}

impl BoundingBox {
    /// Create a new bounding box
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    /// Get center point
    #[must_use]
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Get area
    #[must_use]
    pub fn area(&self) -> f32 {
        self.width * self.height
    }

    /// Check if point is inside the box
    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.width && y >= self.y && y <= self.y + self.height
    }

    /// Expand the bounding box by a factor
    #[must_use]
    pub fn expand(&self, factor: f32) -> Self {
        let expand_w = self.width * (factor - 1.0) / 2.0;
        let expand_h = self.height * (factor - 1.0) / 2.0;
        Self {
            x: self.x - expand_w,
            y: self.y - expand_h,
            width: self.width * factor,
            height: self.height * factor,
        }
    }

    /// Convert to integer coordinates (clamped to image bounds)
    #[must_use]
    pub fn to_rect(&self, img_width: u32, img_height: u32) -> (u32, u32, u32, u32) {
        let x = self.x.max(0.0) as u32;
        let y = self.y.max(0.0) as u32;
        let w = (self.width as u32).min(img_width.saturating_sub(x));
        let h = (self.height as u32).min(img_height.saturating_sub(y));
        (x, y, w, h)
    }

    /// Compute IoU (Intersection over Union) with another box
    #[must_use]
    pub fn iou(&self, other: &BoundingBox) -> f32 {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.width).min(other.x + other.width);
        let y2 = (self.y + self.height).min(other.y + other.height);

        if x2 <= x1 || y2 <= y1 {
            return 0.0;
        }

        let intersection = (x2 - x1) * (y2 - y1);
        let union = self.area() + other.area() - intersection;

        if union > 0.0 {
            intersection / union
        } else {
            0.0
        }
    }
}

/// Combined face detection pipeline
pub struct FaceDetectionPipeline {
    /// Face detector
    detector: FaceDetector,
    /// Landmark detector
    landmark_detector: LandmarkDetector,
    /// Configuration
    config: DetectionConfig,
}

impl FaceDetectionPipeline {
    /// Create a new face detection pipeline
    ///
    /// # Arguments
    ///
    /// * `model_dir` - Directory containing ONNX models
    /// * `config` - Detection configuration
    ///
    /// # Errors
    ///
    /// Returns an error if models cannot be loaded
    pub fn new<P: AsRef<Path>>(model_dir: P, config: DetectionConfig) -> Result<Self> {
        let model_dir = model_dir.as_ref();

        let detection_model = model_dir.join(&config.detection_model);
        let landmark_model = model_dir.join(&config.landmark_model);

        let detector = FaceDetector::load(&detection_model, &config)?;
        let landmark_detector = LandmarkDetector::load(&landmark_model)?;

        Ok(Self {
            detector,
            landmark_detector,
            config,
        })
    }

    /// Detect and process a face from a frame
    ///
    /// This method performs the full detection pipeline:
    /// 1. Detect faces in the frame
    /// 2. Validate single face requirement
    /// 3. Extract landmarks
    /// 4. Align face for embedding
    ///
    /// # Arguments
    ///
    /// * `frame` - Input camera frame
    ///
    /// # Errors
    ///
    /// Returns an error if detection fails or requirements not met
    pub fn process_frame(&self, frame: &Frame) -> Result<ProcessedFace> {
        // Detect faces
        let faces = self.detector.detect(frame)?;

        // Check face count
        if faces.is_empty() {
            return Err(DetectionError::NoFaceDetected.into());
        }

        if faces.len() > self.config.max_faces {
            return Err(DetectionError::MultipleFaces { count: faces.len() }.into());
        }

        // Get the best face (highest confidence)
        let face = faces
            .into_iter()
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap())
            .unwrap();

        // Check confidence threshold
        if face.confidence < self.config.confidence_threshold {
            return Err(DetectionError::LowConfidence {
                confidence: face.confidence,
                threshold: self.config.confidence_threshold,
            }
            .into());
        }

        // Check face size
        let face_size = face.bbox.width.min(face.bbox.height) as u32;
        if face_size < self.config.min_face_size {
            return Err(DetectionError::FaceTooSmall {
                size: face_size,
                min_size: self.config.min_face_size,
            }
            .into());
        }

        // Extract landmarks
        let landmarks = self.landmark_detector.detect(frame, &face.bbox)?;

        // Align face
        let aligned = if self.config.enable_alignment {
            Some(align_face(frame, &landmarks)?)
        } else {
            None
        };

        let face_bbox = face.bbox;

        Ok(ProcessedFace {
            face,
            face_bbox,
            landmarks,
            aligned,
        })
    }

    /// Detect faces without full processing (for liveness checks)
    pub fn detect_faces(&self, frame: &Frame) -> Result<Vec<DetectedFace>> {
        self.detector.detect(frame)
    }

    /// Get landmarks for a specific face region
    pub fn get_landmarks(&self, frame: &Frame, bbox: &BoundingBox) -> Result<FaceLandmarks> {
        self.landmark_detector.detect(frame, bbox)
    }
}

/// Result of face detection pipeline
#[derive(Debug)]
pub struct ProcessedFace {
    /// Detected face with bounding box
    pub face: DetectedFace,
    /// Face bounding box (convenience accessor)
    pub face_bbox: BoundingBox,
    /// Facial landmarks
    pub landmarks: FaceLandmarks,
    /// Aligned face image (if alignment enabled)
    pub aligned: Option<AlignedFace>,
}

impl ProcessedFace {
    /// Get eye landmarks for liveness detection
    #[must_use]
    pub fn left_eye_landmarks(&self) -> &[(f32, f32)] {
        self.landmarks.left_eye()
    }

    /// Get right eye landmarks
    #[must_use]
    pub fn right_eye_landmarks(&self) -> &[(f32, f32)] {
        self.landmarks.right_eye()
    }

    /// Get face center for tracking
    #[must_use]
    pub fn face_center(&self) -> (f32, f32) {
        self.face.bbox.center()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounding_box_center() {
        let bbox = BoundingBox::new(100.0, 100.0, 50.0, 50.0);
        let (cx, cy) = bbox.center();
        assert!((cx - 125.0).abs() < 0.001);
        assert!((cy - 125.0).abs() < 0.001);
    }

    #[test]
    fn test_bounding_box_area() {
        let bbox = BoundingBox::new(0.0, 0.0, 10.0, 20.0);
        assert!((bbox.area() - 200.0).abs() < 0.001);
    }

    #[test]
    fn test_bounding_box_contains() {
        let bbox = BoundingBox::new(10.0, 10.0, 20.0, 20.0);
        assert!(bbox.contains(15.0, 15.0));
        assert!(bbox.contains(10.0, 10.0));
        assert!(!bbox.contains(5.0, 5.0));
        assert!(!bbox.contains(35.0, 35.0));
    }

    #[test]
    fn test_bounding_box_expand() {
        let bbox = BoundingBox::new(100.0, 100.0, 50.0, 50.0);
        let expanded = bbox.expand(1.2);
        assert!((expanded.width - 60.0).abs() < 0.001);
        assert!((expanded.height - 60.0).abs() < 0.001);
    }

    #[test]
    fn test_bounding_box_iou() {
        let bbox1 = BoundingBox::new(0.0, 0.0, 10.0, 10.0);
        let bbox2 = BoundingBox::new(5.0, 5.0, 10.0, 10.0);

        let iou = bbox1.iou(&bbox2);
        // Intersection: 5x5 = 25, Union: 100 + 100 - 25 = 175
        assert!((iou - 25.0 / 175.0).abs() < 0.001);

        // No overlap
        let bbox3 = BoundingBox::new(20.0, 20.0, 10.0, 10.0);
        assert!(bbox1.iou(&bbox3) < 0.001);

        // Same box
        assert!((bbox1.iou(&bbox1) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_bounding_box_to_rect() {
        let bbox = BoundingBox::new(-10.0, -10.0, 100.0, 100.0);
        let (x, y, w, h) = bbox.to_rect(640, 480);
        assert_eq!(x, 0);
        assert_eq!(y, 0);
    }
}
