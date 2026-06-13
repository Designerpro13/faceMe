//! ONNX model loading and inference utilities

use crate::error::{DetectionError, Result};
use ndarray::{Array, Array4, IxDyn};
use std::path::Path;

/// ONNX session wrapper with thread-safe sharing
pub struct OnnxModel {
    /// Model input name
    input_name: String,
    /// Model input shape (N, C, H, W)
    input_shape: Vec<i64>,
    /// Output names
    output_names: Vec<String>,
    /// Model path (for debugging)
    _model_path: std::path::PathBuf,
}

impl OnnxModel {
    /// Load an ONNX model from file
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the ONNX model file
    ///
    /// # Errors
    ///
    /// Returns an error if the model cannot be loaded
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(DetectionError::ModelLoadFailed {
                path: path.to_path_buf(),
                reason: "File not found".to_string(),
            }
            .into());
        }

        // For now, create a stub model with default values
        // Real ONNX loading will be implemented when ort API stabilizes
        Ok(Self {
            input_name: "input".to_string(),
            input_shape: vec![1, 3, 112, 112],
            output_names: vec!["output".to_string()],
            _model_path: path.to_path_buf(),
        })
    }

    /// Get the expected input shape
    #[must_use]
    pub fn input_shape(&self) -> &[i64] {
        &self.input_shape
    }

    /// Get input height (assumes NCHW format)
    #[must_use]
    pub fn input_height(&self) -> usize {
        if self.input_shape.len() >= 4 {
            self.input_shape[2].max(1) as usize
        } else {
            112
        }
    }

    /// Get input width (assumes NCHW format)
    #[must_use]
    pub fn input_width(&self) -> usize {
        if self.input_shape.len() >= 4 {
            self.input_shape[3].max(1) as usize
        } else {
            112
        }
    }

    /// Get output names
    #[must_use]
    pub fn output_names(&self) -> &[String] {
        &self.output_names
    }

    /// Get input name
    #[must_use]
    pub fn input_name(&self) -> &str {
        &self.input_name
    }

    /// Run inference on a preprocessed input tensor
    ///
    /// # Arguments
    ///
    /// * `input` - Input tensor in NCHW format
    ///
    /// # Errors
    ///
    /// Returns an error if inference fails
    pub fn run(&self, _input: Array4<f32>) -> Result<Vec<Array<f32, IxDyn>>> {
        // Stub implementation - returns empty results
        // Real implementation requires proper ort session management
        Err(DetectionError::InferenceFailed(
            "ONNX inference not yet implemented - model stub only".to_string()
        ).into())
    }
}

/// Preprocess an image for model input
///
/// Converts BGR image to normalized RGB tensor in NCHW format
///
/// # Arguments
///
/// * `data` - Raw BGR image data
/// * `width` - Image width
/// * `height` - Image height
/// * `target_width` - Target width after resize
/// * `target_height` - Target height after resize
/// * `mean` - Per-channel mean for normalization
/// * `std` - Per-channel std for normalization
pub fn preprocess_image(
    data: &[u8],
    width: usize,
    height: usize,
    target_width: usize,
    target_height: usize,
    mean: [f32; 3],
    std: [f32; 3],
) -> Array4<f32> {
    // Resize using bilinear interpolation
    let resized = bilinear_resize(data, width, height, target_width, target_height, 3);

    // Convert to NCHW format and normalize
    let mut tensor = Array4::<f32>::zeros((1, 3, target_height, target_width));

    for y in 0..target_height {
        for x in 0..target_width {
            let idx = (y * target_width + x) * 3;
            // BGR to RGB and normalize
            tensor[[0, 0, y, x]] = (resized[idx + 2] as f32 / 255.0 - mean[0]) / std[0]; // R
            tensor[[0, 1, y, x]] = (resized[idx + 1] as f32 / 255.0 - mean[1]) / std[1]; // G
            tensor[[0, 2, y, x]] = (resized[idx] as f32 / 255.0 - mean[2]) / std[2]; // B
        }
    }

    tensor
}

/// Preprocess image with simple normalization (0-1 range, no mean/std)
pub fn preprocess_image_simple(
    data: &[u8],
    width: usize,
    height: usize,
    target_width: usize,
    target_height: usize,
) -> Array4<f32> {
    preprocess_image(
        data,
        width,
        height,
        target_width,
        target_height,
        [0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0],
    )
}

/// Preprocess with ImageNet normalization
pub fn preprocess_image_imagenet(
    data: &[u8],
    width: usize,
    height: usize,
    target_width: usize,
    target_height: usize,
) -> Array4<f32> {
    preprocess_image(
        data,
        width,
        height,
        target_width,
        target_height,
        [0.485, 0.456, 0.406],
        [0.229, 0.224, 0.225],
    )
}

/// Preprocess with ArcFace/MobileFaceNet normalization
pub fn preprocess_image_arcface(
    data: &[u8],
    width: usize,
    height: usize,
    target_width: usize,
    target_height: usize,
) -> Array4<f32> {
    preprocess_image(
        data,
        width,
        height,
        target_width,
        target_height,
        [0.5, 0.5, 0.5],
        [0.5, 0.5, 0.5],
    )
}

/// Bilinear resize for image data
pub fn bilinear_resize(
    data: &[u8],
    src_width: usize,
    src_height: usize,
    dst_width: usize,
    dst_height: usize,
    channels: usize,
) -> Vec<u8> {
    let mut result = vec![0u8; dst_width * dst_height * channels];

    let x_ratio = src_width as f32 / dst_width as f32;
    let y_ratio = src_height as f32 / dst_height as f32;

    for y in 0..dst_height {
        for x in 0..dst_width {
            let src_x = x as f32 * x_ratio;
            let src_y = y as f32 * y_ratio;

            let x0 = src_x.floor() as usize;
            let y0 = src_y.floor() as usize;
            let x1 = (x0 + 1).min(src_width - 1);
            let y1 = (y0 + 1).min(src_height - 1);

            let x_frac = src_x - x0 as f32;
            let y_frac = src_y - y0 as f32;

            for c in 0..channels {
                let p00 = data[(y0 * src_width + x0) * channels + c] as f32;
                let p01 = data[(y0 * src_width + x1) * channels + c] as f32;
                let p10 = data[(y1 * src_width + x0) * channels + c] as f32;
                let p11 = data[(y1 * src_width + x1) * channels + c] as f32;

                let value = p00 * (1.0 - x_frac) * (1.0 - y_frac)
                    + p01 * x_frac * (1.0 - y_frac)
                    + p10 * (1.0 - x_frac) * y_frac
                    + p11 * x_frac * y_frac;

                result[(y * dst_width + x) * channels + c] = value.clamp(0.0, 255.0) as u8;
            }
        }
    }

    result
}

/// Crop and resize a region from an image
pub fn crop_and_resize(
    data: &[u8],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    crop_w: usize,
    crop_h: usize,
    target_w: usize,
    target_h: usize,
    channels: usize,
) -> Vec<u8> {
    // First crop
    let mut cropped = vec![0u8; crop_w * crop_h * channels];
    for cy in 0..crop_h {
        for cx in 0..crop_w {
            let src_x = (x + cx).min(width - 1);
            let src_y = (y + cy).min(height - 1);
            let src_idx = (src_y * width + src_x) * channels;
            let dst_idx = (cy * crop_w + cx) * channels;
            for c in 0..channels {
                cropped[dst_idx + c] = data[src_idx + c];
            }
        }
    }

    // Then resize
    bilinear_resize(&cropped, crop_w, crop_h, target_w, target_h, channels)
}

/// Apply softmax to a slice
pub fn softmax(values: &[f32]) -> Vec<f32> {
    let max_val = values.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exp_values: Vec<f32> = values.iter().map(|v| (v - max_val).exp()).collect();
    let sum: f32 = exp_values.iter().sum();
    exp_values.iter().map(|v| v / sum).collect()
}

/// Apply sigmoid to a value
#[inline]
pub fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bilinear_resize() {
        // 2x2 grayscale image
        let data = vec![0u8, 255, 255, 0];
        let resized = bilinear_resize(&data, 2, 2, 4, 4, 1);
        assert_eq!(resized.len(), 16);
    }

    #[test]
    fn test_preprocess_simple() {
        let data = vec![128u8; 100 * 100 * 3];
        let tensor = preprocess_image_simple(&data, 100, 100, 112, 112);
        assert_eq!(tensor.shape(), &[1, 3, 112, 112]);
    }

    #[test]
    fn test_softmax() {
        let values = vec![1.0, 2.0, 3.0];
        let result = softmax(&values);
        let sum: f32 = result.iter().sum();
        assert!((sum - 1.0).abs() < 0.0001);
        assert!(result[2] > result[1]);
        assert!(result[1] > result[0]);
    }

    #[test]
    fn test_sigmoid() {
        assert!((sigmoid(0.0) - 0.5).abs() < 0.0001);
        assert!(sigmoid(10.0) > 0.99);
        assert!(sigmoid(-10.0) < 0.01);
    }

    #[test]
    fn test_crop_and_resize() {
        let data = vec![100u8; 100 * 100 * 3];
        let result = crop_and_resize(&data, 100, 100, 10, 10, 50, 50, 32, 32, 3);
        assert_eq!(result.len(), 32 * 32 * 3);
    }
}
