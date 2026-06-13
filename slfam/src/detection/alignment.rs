//! Face alignment for consistent embedding generation

use super::landmarks::{euclidean_distance, FaceLandmarks};
use super::onnx::bilinear_resize;
use crate::camera::Frame;
use crate::error::{DetectionError, Result};
use zeroize::Zeroize;

/// Standard alignment target points for 112x112 face crop
/// Based on ArcFace/InsightFace alignment
const REFERENCE_LANDMARKS_112: [(f32, f32); 5] = [
    (38.2946, 51.6963),  // Left eye
    (73.5318, 51.5014),  // Right eye
    (56.0252, 71.7366),  // Nose tip
    (41.5493, 92.3655),  // Left mouth corner
    (70.7299, 92.2041),  // Right mouth corner
];

/// Aligned face image ready for embedding generation
#[derive(Debug, Clone)]
pub struct AlignedFace {
    /// Aligned face image data (BGR format)
    data: Vec<u8>,
    /// Image width
    width: u32,
    /// Image height
    height: u32,
}

impl AlignedFace {
    /// Create a new aligned face
    pub fn new(data: Vec<u8>, width: u32, height: u32) -> Self {
        Self { data, width, height }
    }

    /// Get image data
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get image width
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Get image height
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Consume and return data
    pub fn into_data(mut self) -> Vec<u8> {
        std::mem::take(&mut self.data)
    }
}

impl Drop for AlignedFace {
    fn drop(&mut self) {
        self.data.zeroize();
    }
}

/// Align a face using 5-point landmarks
///
/// This applies a similarity transform (rotation, scale, translation)
/// to normalize the face to a standard position for consistent embeddings.
///
/// # Arguments
///
/// * `frame` - Input frame
/// * `landmarks` - Detected facial landmarks
///
/// # Errors
///
/// Returns an error if alignment fails
pub fn align_face(frame: &Frame, landmarks: &FaceLandmarks) -> Result<AlignedFace> {
    let five_points = landmarks.five_point().ok_or_else(|| {
        DetectionError::AlignmentFailed("Cannot extract 5-point landmarks".to_string())
    })?;

    align_face_5point(frame, &five_points, 112, 112)
}

/// Align face using 5 landmark points
pub fn align_face_5point(
    frame: &Frame,
    src_points: &[(f32, f32); 5],
    output_width: u32,
    output_height: u32,
) -> Result<AlignedFace> {
    let frame_bgr = frame.to_bgr24()?;
    let data = frame_bgr.data();
    let width = frame.width() as usize;
    let height = frame.height() as usize;

    // Compute similarity transform from source to reference points
    let transform = compute_similarity_transform(src_points, &REFERENCE_LANDMARKS_112)?;

    // Apply transform
    let aligned = apply_similarity_transform(
        data,
        width,
        height,
        &transform,
        output_width as usize,
        output_height as usize,
    );

    Ok(AlignedFace::new(aligned, output_width, output_height))
}

/// 2x3 similarity transform matrix [a, b, tx; -b, a, ty]
#[derive(Debug, Clone, Copy)]
struct SimilarityTransform {
    /// Scale * cos(angle)
    a: f32,
    /// Scale * sin(angle)
    b: f32,
    /// Translation X
    tx: f32,
    /// Translation Y
    ty: f32,
}

impl SimilarityTransform {
    /// Apply transform to a point
    fn transform_point(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x - self.b * y + self.tx,
            self.b * x + self.a * y + self.ty,
        )
    }

    /// Get inverse transform
    fn inverse(&self) -> Self {
        let det = self.a * self.a + self.b * self.b;
        let a_inv = self.a / det;
        let b_inv = -self.b / det;
        let tx_inv = -(a_inv * self.tx - b_inv * self.ty);
        let ty_inv = -(b_inv * self.tx + a_inv * self.ty);

        Self {
            a: a_inv,
            b: b_inv,
            tx: tx_inv,
            ty: ty_inv,
        }
    }
}

/// Compute similarity transform using least squares
fn compute_similarity_transform(
    src: &[(f32, f32); 5],
    dst: &[(f32, f32); 5],
) -> Result<SimilarityTransform> {
    // Compute centroids
    let src_cx: f32 = src.iter().map(|p| p.0).sum::<f32>() / 5.0;
    let src_cy: f32 = src.iter().map(|p| p.1).sum::<f32>() / 5.0;
    let dst_cx: f32 = dst.iter().map(|p| p.0).sum::<f32>() / 5.0;
    let dst_cy: f32 = dst.iter().map(|p| p.1).sum::<f32>() / 5.0;

    // Compute covariance terms
    let mut sxx = 0.0f32;
    let mut sxy = 0.0f32;
    let mut syx = 0.0f32;
    let mut syy = 0.0f32;
    let mut src_var = 0.0f32;

    for i in 0..5 {
        let sx = src[i].0 - src_cx;
        let sy = src[i].1 - src_cy;
        let dx = dst[i].0 - dst_cx;
        let dy = dst[i].1 - dst_cy;

        sxx += sx * dx;
        sxy += sx * dy;
        syx += sy * dx;
        syy += sy * dy;
        src_var += sx * sx + sy * sy;
    }

    if src_var < 1e-6 {
        return Err(DetectionError::AlignmentFailed(
            "Source points have zero variance".to_string(),
        )
        .into());
    }

    // Compute optimal rotation and scale
    let a = (sxx + syy) / src_var;
    let b = (syx - sxy) / src_var;

    // Compute translation
    let tx = dst_cx - a * src_cx + b * src_cy;
    let ty = dst_cy - b * src_cx - a * src_cy;

    Ok(SimilarityTransform { a, b, tx, ty })
}

/// Apply similarity transform with bilinear interpolation
fn apply_similarity_transform(
    src_data: &[u8],
    src_width: usize,
    src_height: usize,
    transform: &SimilarityTransform,
    dst_width: usize,
    dst_height: usize,
) -> Vec<u8> {
    let channels = 3;
    let mut dst_data = vec![0u8; dst_width * dst_height * channels];

    // Use inverse transform to map destination to source
    let inv = transform.inverse();

    for y in 0..dst_height {
        for x in 0..dst_width {
            // Map destination point to source
            let (src_x, src_y) = inv.transform_point(x as f32, y as f32);

            // Bilinear interpolation
            if src_x >= 0.0
                && src_x < (src_width - 1) as f32
                && src_y >= 0.0
                && src_y < (src_height - 1) as f32
            {
                let x0 = src_x.floor() as usize;
                let y0 = src_y.floor() as usize;
                let x1 = x0 + 1;
                let y1 = y0 + 1;

                let fx = src_x - x0 as f32;
                let fy = src_y - y0 as f32;

                for c in 0..channels {
                    let p00 = src_data[(y0 * src_width + x0) * channels + c] as f32;
                    let p01 = src_data[(y0 * src_width + x1) * channels + c] as f32;
                    let p10 = src_data[(y1 * src_width + x0) * channels + c] as f32;
                    let p11 = src_data[(y1 * src_width + x1) * channels + c] as f32;

                    let value = p00 * (1.0 - fx) * (1.0 - fy)
                        + p01 * fx * (1.0 - fy)
                        + p10 * (1.0 - fx) * fy
                        + p11 * fx * fy;

                    dst_data[(y * dst_width + x) * channels + c] = value.clamp(0.0, 255.0) as u8;
                }
            }
        }
    }

    dst_data
}

/// Simple crop-based alignment (no rotation, just crop and resize)
pub fn simple_align(
    frame: &Frame,
    landmarks: &FaceLandmarks,
    output_size: u32,
) -> Result<AlignedFace> {
    let frame_bgr = frame.to_bgr24()?;
    let data = frame_bgr.data();
    let width = frame.width() as usize;
    let height = frame.height() as usize;

    // Get face bounding box from landmarks
    let points = landmarks.points();
    if points.is_empty() {
        return Err(DetectionError::AlignmentFailed("No landmarks".to_string()).into());
    }

    let min_x = points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
    let max_x = points.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
    let min_y = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
    let max_y = points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);

    // Add padding
    let face_w = max_x - min_x;
    let face_h = max_y - min_y;
    let padding = 0.2;

    let crop_x = (min_x - face_w * padding).max(0.0) as usize;
    let crop_y = (min_y - face_h * padding).max(0.0) as usize;
    let crop_w = (face_w * (1.0 + 2.0 * padding)) as usize;
    let crop_h = (face_h * (1.0 + 2.0 * padding)) as usize;

    // Make it square
    let crop_size = crop_w.max(crop_h);

    // Crop and resize
    let aligned = super::onnx::crop_and_resize(
        data,
        width,
        height,
        crop_x,
        crop_y,
        crop_size.min(width - crop_x),
        crop_size.min(height - crop_y),
        output_size as usize,
        output_size as usize,
        3,
    );

    Ok(AlignedFace::new(aligned, output_size, output_size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_similarity_transform_identity() {
        let points = [
            (10.0, 20.0),
            (30.0, 20.0),
            (20.0, 35.0),
            (15.0, 45.0),
            (25.0, 45.0),
        ];

        let transform = compute_similarity_transform(&points, &points).unwrap();

        // Should be close to identity (a=1, b=0, tx=0, ty=0)
        assert!((transform.a - 1.0).abs() < 0.01);
        assert!(transform.b.abs() < 0.01);
        assert!(transform.tx.abs() < 1.0);
        assert!(transform.ty.abs() < 1.0);
    }

    #[test]
    fn test_similarity_transform_translation() {
        let src = [
            (0.0, 0.0),
            (10.0, 0.0),
            (5.0, 5.0),
            (2.0, 10.0),
            (8.0, 10.0),
        ];
        let dst = [
            (100.0, 100.0),
            (110.0, 100.0),
            (105.0, 105.0),
            (102.0, 110.0),
            (108.0, 110.0),
        ];

        let transform = compute_similarity_transform(&src, &dst).unwrap();

        // Should translate by (100, 100)
        assert!((transform.a - 1.0).abs() < 0.01);
        assert!(transform.b.abs() < 0.01);
        assert!((transform.tx - 100.0).abs() < 1.0);
        assert!((transform.ty - 100.0).abs() < 1.0);
    }

    #[test]
    fn test_transform_inverse() {
        let transform = SimilarityTransform {
            a: 0.866,  // cos(30°)
            b: 0.5,    // sin(30°)
            tx: 50.0,
            ty: 100.0,
        };

        let inv = transform.inverse();
        let composed = SimilarityTransform {
            a: transform.a * inv.a + transform.b * inv.b,
            b: transform.b * inv.a - transform.a * inv.b,
            tx: transform.a * inv.tx - transform.b * inv.ty + transform.tx,
            ty: transform.b * inv.tx + transform.a * inv.ty + transform.ty,
        };

        // Composed should be close to identity
        assert!((composed.a - 1.0).abs() < 0.01);
        assert!(composed.b.abs() < 0.01);
        assert!(composed.tx.abs() < 1.0);
        assert!(composed.ty.abs() < 1.0);
    }

    #[test]
    fn test_aligned_face_creation() {
        let data = vec![128u8; 112 * 112 * 3];
        let face = AlignedFace::new(data, 112, 112);
        
        assert_eq!(face.width(), 112);
        assert_eq!(face.height(), 112);
        assert_eq!(face.data().len(), 112 * 112 * 3);
    }

    #[test]
    fn test_reference_landmarks() {
        // Verify reference landmarks are valid
        for (x, y) in REFERENCE_LANDMARKS_112.iter() {
            assert!(*x >= 0.0 && *x < 112.0);
            assert!(*y >= 0.0 && *y < 112.0);
        }

        // Eyes should be above nose
        assert!(REFERENCE_LANDMARKS_112[0].1 < REFERENCE_LANDMARKS_112[2].1);
        assert!(REFERENCE_LANDMARKS_112[1].1 < REFERENCE_LANDMARKS_112[2].1);

        // Mouth should be below nose
        assert!(REFERENCE_LANDMARKS_112[3].1 > REFERENCE_LANDMARKS_112[2].1);
        assert!(REFERENCE_LANDMARKS_112[4].1 > REFERENCE_LANDMARKS_112[2].1);
    }
}
