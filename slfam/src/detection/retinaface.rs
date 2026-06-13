//! RetinaFace detector implementation

use super::onnx::{OnnxModel, preprocess_image_simple, sigmoid, softmax};
use super::BoundingBox;
use crate::camera::Frame;
use crate::config::DetectionConfig;
use crate::error::{DetectionError, Result};
use ndarray::{Array2, Array4, Axis};
use std::path::Path;

/// A detected face with bounding box and confidence
#[derive(Debug, Clone)]
pub struct DetectedFace {
    /// Bounding box of the face
    pub bbox: BoundingBox,
    /// Detection confidence (0.0 - 1.0)
    pub confidence: f32,
    /// Key points (eyes, nose, mouth corners) if available
    pub keypoints: Option<Vec<(f32, f32)>>,
}

/// RetinaFace-based face detector
pub struct FaceDetector {
    /// ONNX model
    model: OnnxModel,
    /// Confidence threshold
    confidence_threshold: f32,
    /// NMS IoU threshold
    nms_threshold: f32,
    /// Input size
    input_size: (usize, usize),
    /// Anchor configuration
    anchors: AnchorConfig,
}

/// Anchor box configuration for RetinaFace
struct AnchorConfig {
    /// Feature map strides
    strides: Vec<usize>,
    /// Min sizes for each stride
    min_sizes: Vec<Vec<usize>>,
    /// Variance for bbox decoding
    variance: [f32; 2],
}

impl Default for AnchorConfig {
    fn default() -> Self {
        Self {
            strides: vec![8, 16, 32],
            min_sizes: vec![vec![16, 32], vec![64, 128], vec![256, 512]],
            variance: [0.1, 0.2],
        }
    }
}

impl FaceDetector {
    /// Load a RetinaFace model
    ///
    /// # Arguments
    ///
    /// * `model_path` - Path to the ONNX model
    /// * `config` - Detection configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the model cannot be loaded
    pub fn load<P: AsRef<Path>>(model_path: P, config: &DetectionConfig) -> Result<Self> {
        let model = OnnxModel::load(model_path)?;
        
        let input_height = model.input_height();
        let input_width = model.input_width();

        Ok(Self {
            model,
            confidence_threshold: config.confidence_threshold,
            nms_threshold: 0.4,
            input_size: (input_width, input_height),
            anchors: AnchorConfig::default(),
        })
    }

    /// Detect faces in a frame
    ///
    /// # Arguments
    ///
    /// * `frame` - Input camera frame
    ///
    /// # Errors
    ///
    /// Returns an error if detection fails
    pub fn detect(&self, frame: &Frame) -> Result<Vec<DetectedFace>> {
        // Convert frame to BGR if needed
        let frame_bgr = frame.to_bgr24()?;
        let data = frame_bgr.data();
        let width = frame.width() as usize;
        let height = frame.height() as usize;

        // Preprocess
        let input = preprocess_image_simple(
            data,
            width,
            height,
            self.input_size.0,
            self.input_size.1,
        );

        // Run inference
        let outputs = self.model.run(input)?;

        // Parse outputs (RetinaFace has multiple outputs for different strides)
        let detections = self.parse_outputs(&outputs, width as f32, height as f32)?;

        // Apply NMS
        let final_detections = self.non_max_suppression(detections);

        Ok(final_detections)
    }

    /// Parse model outputs to detections
    fn parse_outputs(
        &self,
        outputs: &[ndarray::Array<f32, ndarray::IxDyn>],
        orig_width: f32,
        orig_height: f32,
    ) -> Result<Vec<DetectedFace>> {
        let mut detections = Vec::new();

        // Scale factors for converting back to original image size
        let scale_x = orig_width / self.input_size.0 as f32;
        let scale_y = orig_height / self.input_size.1 as f32;

        // Generate anchors and decode outputs
        // This is a simplified version - actual implementation depends on model variant
        
        if outputs.is_empty() {
            return Ok(detections);
        }

        // Assuming single output with format [N, num_anchors, 15] or similar
        // where each anchor has: [x, y, w, h, confidence, landmark_x * 5, landmark_y * 5]
        
        let output = &outputs[0];
        let shape = output.shape();
        
        // Handle different output formats
        if shape.len() >= 2 {
            let num_detections = if shape.len() == 3 { shape[1] } else { shape[0] };
            
            for i in 0..num_detections {
                let (bbox, conf, keypoints) = if shape.len() == 3 {
                    self.decode_detection_3d(output, i, scale_x, scale_y)
                } else {
                    self.decode_detection_2d(output, i, scale_x, scale_y)
                };
                
                if conf >= self.confidence_threshold {
                    detections.push(DetectedFace {
                        bbox,
                        confidence: conf,
                        keypoints,
                    });
                }
            }
        }

        Ok(detections)
    }

    /// Decode a single detection from 3D output tensor
    fn decode_detection_3d(
        &self,
        output: &ndarray::Array<f32, ndarray::IxDyn>,
        idx: usize,
        scale_x: f32,
        scale_y: f32,
    ) -> (BoundingBox, f32, Option<Vec<(f32, f32)>>) {
        let row = output.slice(ndarray::s![0, idx, ..]);
        self.decode_row(row.as_slice().unwrap_or(&[]), scale_x, scale_y)
    }

    /// Decode a single detection from 2D output tensor
    fn decode_detection_2d(
        &self,
        output: &ndarray::Array<f32, ndarray::IxDyn>,
        idx: usize,
        scale_x: f32,
        scale_y: f32,
    ) -> (BoundingBox, f32, Option<Vec<(f32, f32)>>) {
        let row = output.slice(ndarray::s![idx, ..]);
        self.decode_row(row.as_slice().unwrap_or(&[]), scale_x, scale_y)
    }

    /// Decode a single row of detection data
    fn decode_row(
        &self,
        row: &[f32],
        scale_x: f32,
        scale_y: f32,
    ) -> (BoundingBox, f32, Option<Vec<(f32, f32)>>) {
        if row.len() < 5 {
            return (BoundingBox::new(0.0, 0.0, 0.0, 0.0), 0.0, None);
        }

        // Decode bounding box (x_center, y_center, width, height format)
        let x_center = row[0] * scale_x;
        let y_center = row[1] * scale_y;
        let w = row[2] * scale_x;
        let h = row[3] * scale_y;
        let conf = sigmoid(row[4]);

        let bbox = BoundingBox::new(
            x_center - w / 2.0,
            y_center - h / 2.0,
            w,
            h,
        );

        // Decode keypoints if present
        let keypoints = if row.len() >= 15 {
            Some(vec![
                (row[5] * scale_x, row[6] * scale_y),   // Left eye
                (row[7] * scale_x, row[8] * scale_y),   // Right eye
                (row[9] * scale_x, row[10] * scale_y),  // Nose
                (row[11] * scale_x, row[12] * scale_y), // Left mouth
                (row[13] * scale_x, row[14] * scale_y), // Right mouth
            ])
        } else {
            None
        };

        (bbox, conf, keypoints)
    }

    /// Apply Non-Maximum Suppression
    fn non_max_suppression(&self, mut detections: Vec<DetectedFace>) -> Vec<DetectedFace> {
        if detections.is_empty() {
            return detections;
        }

        // Sort by confidence (descending)
        detections.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        let mut keep = Vec::new();
        let mut suppressed = vec![false; detections.len()];

        for i in 0..detections.len() {
            if suppressed[i] {
                continue;
            }

            keep.push(detections[i].clone());

            for j in (i + 1)..detections.len() {
                if suppressed[j] {
                    continue;
                }

                let iou = detections[i].bbox.iou(&detections[j].bbox);
                if iou > self.nms_threshold {
                    suppressed[j] = true;
                }
            }
        }

        keep
    }

    /// Set confidence threshold
    pub fn set_confidence_threshold(&mut self, threshold: f32) {
        self.confidence_threshold = threshold.clamp(0.0, 1.0);
    }

    /// Set NMS threshold
    pub fn set_nms_threshold(&mut self, threshold: f32) {
        self.nms_threshold = threshold.clamp(0.0, 1.0);
    }
}

/// Simple face detector using cascade-like approach (fallback when no ONNX model)
pub struct SimpleFaceDetector {
    /// Minimum face size
    min_size: u32,
    /// Scale factor for multi-scale detection
    scale_factor: f32,
}

impl SimpleFaceDetector {
    /// Create a new simple face detector
    pub fn new(min_size: u32) -> Self {
        Self {
            min_size,
            scale_factor: 1.2,
        }
    }

    /// Detect faces using simple skin color and shape heuristics
    ///
    /// This is a fallback detector for testing without ONNX models
    pub fn detect(&self, frame: &Frame) -> Result<Vec<DetectedFace>> {
        let frame_bgr = frame.to_bgr24()?;
        let data = frame_bgr.data();
        let width = frame.width();
        let height = frame.height();

        // Simple skin color detection
        let skin_mask = self.detect_skin(data, width as usize, height as usize);

        // Find connected components that might be faces
        let candidates = self.find_face_candidates(&skin_mask, width as usize, height as usize);

        Ok(candidates)
    }

    /// Detect skin-colored pixels
    fn detect_skin(&self, data: &[u8], width: usize, height: usize) -> Vec<bool> {
        let mut mask = vec![false; width * height];

        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) * 3;
                let b = data[idx] as f32;
                let g = data[idx + 1] as f32;
                let r = data[idx + 2] as f32;

                // Simple skin color detection in RGB space
                let is_skin = r > 95.0
                    && g > 40.0
                    && b > 20.0
                    && r > g
                    && r > b
                    && (r - g).abs() > 15.0
                    && r.max(g).max(b) - r.min(g).min(b) > 15.0;

                mask[y * width + x] = is_skin;
            }
        }

        mask
    }

    /// Find face candidates from skin mask
    fn find_face_candidates(
        &self,
        mask: &[bool],
        width: usize,
        height: usize,
    ) -> Vec<DetectedFace> {
        let mut faces = Vec::new();

        // Simple sliding window approach
        let window_sizes = [80, 120, 160, 200, 250];

        for &window_size in &window_sizes {
            if window_size < self.min_size as usize {
                continue;
            }

            let step = window_size / 4;

            for y in (0..height.saturating_sub(window_size)).step_by(step) {
                for x in (0..width.saturating_sub(window_size)).step_by(step) {
                    let skin_ratio = self.compute_skin_ratio(mask, width, x, y, window_size);

                    // Face-like regions have 20-60% skin
                    if skin_ratio > 0.2 && skin_ratio < 0.6 {
                        let confidence = 0.5 + (0.4 - (skin_ratio - 0.4).abs()) * 0.5;

                        faces.push(DetectedFace {
                            bbox: BoundingBox::new(
                                x as f32,
                                y as f32,
                                window_size as f32,
                                window_size as f32,
                            ),
                            confidence,
                            keypoints: None,
                        });
                    }
                }
            }
        }

        // Apply NMS
        self.simple_nms(faces)
    }

    /// Compute ratio of skin pixels in a window
    fn compute_skin_ratio(
        &self,
        mask: &[bool],
        width: usize,
        x: usize,
        y: usize,
        size: usize,
    ) -> f32 {
        let mut count = 0;
        let total = size * size;

        for dy in 0..size {
            for dx in 0..size {
                if mask[(y + dy) * width + (x + dx)] {
                    count += 1;
                }
            }
        }

        count as f32 / total as f32
    }

    /// Simple NMS for detected faces
    fn simple_nms(&self, mut faces: Vec<DetectedFace>) -> Vec<DetectedFace> {
        if faces.is_empty() {
            return faces;
        }

        faces.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        let mut keep = Vec::new();
        let mut suppressed = vec![false; faces.len()];

        for i in 0..faces.len() {
            if suppressed[i] {
                continue;
            }

            keep.push(faces[i].clone());

            for j in (i + 1)..faces.len() {
                if !suppressed[j] && faces[i].bbox.iou(&faces[j].bbox) > 0.3 {
                    suppressed[j] = true;
                }
            }
        }

        keep
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detected_face_creation() {
        let face = DetectedFace {
            bbox: BoundingBox::new(100.0, 100.0, 50.0, 60.0),
            confidence: 0.95,
            keypoints: None,
        };

        assert_eq!(face.bbox.width, 50.0);
        assert!(face.confidence > 0.9);
    }

    #[test]
    fn test_nms() {
        let detector = SimpleFaceDetector::new(80);

        let faces = vec![
            DetectedFace {
                bbox: BoundingBox::new(100.0, 100.0, 100.0, 100.0),
                confidence: 0.9,
                keypoints: None,
            },
            DetectedFace {
                bbox: BoundingBox::new(110.0, 110.0, 100.0, 100.0),
                confidence: 0.8,
                keypoints: None,
            },
            DetectedFace {
                bbox: BoundingBox::new(300.0, 300.0, 100.0, 100.0),
                confidence: 0.85,
                keypoints: None,
            },
        ];

        let result = detector.simple_nms(faces);
        // First two should be merged (high overlap), third kept separate
        assert_eq!(result.len(), 2);
        assert!(result[0].confidence >= result[1].confidence);
    }

    #[test]
    fn test_anchor_config_default() {
        let config = AnchorConfig::default();
        assert_eq!(config.strides.len(), 3);
        assert_eq!(config.variance, [0.1, 0.2]);
    }
}
