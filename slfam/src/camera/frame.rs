//! Frame data structures and handling

use crate::error::{CameraError, Result};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use zeroize::Zeroize;

/// Frame pixel format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FrameFormat {
    /// RGB 24-bit (8 bits per channel)
    Rgb24,
    /// BGR 24-bit (8 bits per channel, OpenCV native)
    Bgr24,
    /// RGBA 32-bit
    Rgba32,
    /// Grayscale 8-bit
    Gray8,
    /// Grayscale 16-bit (for IR depth data)
    Gray16,
    /// YUV422 (YUYV)
    Yuyv,
    /// MJPEG compressed
    Mjpeg,
    /// NV12 (YUV420 semi-planar)
    Nv12,
}

impl FrameFormat {
    /// Get bytes per pixel for this format
    #[must_use]
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            FrameFormat::Rgb24 | FrameFormat::Bgr24 => 3,
            FrameFormat::Rgba32 => 4,
            FrameFormat::Gray8 => 1,
            FrameFormat::Gray16 => 2,
            FrameFormat::Yuyv => 2,
            FrameFormat::Mjpeg => 0, // Variable
            FrameFormat::Nv12 => 0,  // Planar format
        }
    }

    /// Check if format is compressed
    #[must_use]
    pub fn is_compressed(&self) -> bool {
        matches!(self, FrameFormat::Mjpeg)
    }

    /// Check if format is grayscale
    #[must_use]
    pub fn is_grayscale(&self) -> bool {
        matches!(self, FrameFormat::Gray8 | FrameFormat::Gray16)
    }
}

/// A captured video frame
pub struct Frame {
    /// Raw pixel data
    data: Vec<u8>,
    /// Frame width in pixels
    width: u32,
    /// Frame height in pixels
    height: u32,
    /// Pixel format
    format: FrameFormat,
    /// Capture timestamp
    timestamp: Instant,
    /// Frame sequence number
    sequence: u64,
}

impl Frame {
    /// Create a new frame
    ///
    /// # Arguments
    ///
    /// * `data` - Raw pixel data
    /// * `width` - Frame width
    /// * `height` - Frame height
    /// * `format` - Pixel format
    /// * `sequence` - Sequence number
    pub fn new(data: Vec<u8>, width: u32, height: u32, format: FrameFormat, sequence: u64) -> Self {
        Self {
            data,
            width,
            height,
            format,
            timestamp: Instant::now(),
            sequence,
        }
    }

    /// Create a frame from raw bytes with validation
    ///
    /// # Arguments
    ///
    /// * `data` - Raw pixel data
    /// * `width` - Expected width
    /// * `height` - Expected height
    /// * `format` - Pixel format
    /// * `sequence` - Sequence number
    ///
    /// # Errors
    ///
    /// Returns an error if the data size doesn't match expected dimensions
    pub fn from_bytes(
        data: Vec<u8>,
        width: u32,
        height: u32,
        format: FrameFormat,
        sequence: u64,
    ) -> Result<Self> {
        let expected_size = if !format.is_compressed() {
            (width as usize) * (height as usize) * format.bytes_per_pixel()
        } else {
            0 // Compressed formats have variable size
        };

        if expected_size > 0 && data.len() != expected_size {
            return Err(CameraError::InvalidFormat {
                expected: format!("{} bytes", expected_size),
                actual: format!("{} bytes", data.len()),
            }.into());
        }

        Ok(Self::new(data, width, height, format, sequence))
    }

    /// Get raw pixel data
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get mutable pixel data
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Consume frame and return data
    #[must_use]
    pub fn into_data(mut self) -> Vec<u8> {
        std::mem::take(&mut self.data)
    }

    /// Frame width in pixels
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Frame height in pixels
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Pixel format
    #[must_use]
    pub fn format(&self) -> FrameFormat {
        self.format
    }

    /// Capture timestamp
    #[must_use]
    pub fn timestamp(&self) -> Instant {
        self.timestamp
    }

    /// Time since capture
    #[must_use]
    pub fn age(&self) -> Duration {
        self.timestamp.elapsed()
    }

    /// Frame sequence number
    #[must_use]
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Total number of pixels
    #[must_use]
    pub fn pixel_count(&self) -> usize {
        (self.width as usize) * (self.height as usize)
    }

    /// Convert to grayscale
    ///
    /// # Errors
    ///
    /// Returns an error if conversion is not supported for this format
    pub fn to_grayscale(&self) -> Result<Frame> {
        match self.format {
            FrameFormat::Gray8 | FrameFormat::Gray16 => {
                // Already grayscale, clone
                Ok(Frame::new(
                    self.data.clone(),
                    self.width,
                    self.height,
                    self.format,
                    self.sequence,
                ))
            }
            FrameFormat::Rgb24 => {
                let mut gray_data = Vec::with_capacity(self.pixel_count());
                for pixel in self.data.chunks(3) {
                    // Standard luminosity formula
                    let gray = (0.299 * pixel[0] as f32
                        + 0.587 * pixel[1] as f32
                        + 0.114 * pixel[2] as f32) as u8;
                    gray_data.push(gray);
                }
                Ok(Frame::new(
                    gray_data,
                    self.width,
                    self.height,
                    FrameFormat::Gray8,
                    self.sequence,
                ))
            }
            FrameFormat::Bgr24 => {
                let mut gray_data = Vec::with_capacity(self.pixel_count());
                for pixel in self.data.chunks(3) {
                    // BGR order
                    let gray = (0.114 * pixel[0] as f32
                        + 0.587 * pixel[1] as f32
                        + 0.299 * pixel[2] as f32) as u8;
                    gray_data.push(gray);
                }
                Ok(Frame::new(
                    gray_data,
                    self.width,
                    self.height,
                    FrameFormat::Gray8,
                    self.sequence,
                ))
            }
            _ => Err(CameraError::InvalidFormat {
                expected: "RGB24 or BGR24".to_string(),
                actual: format!("{:?}", self.format),
            }.into()),
        }
    }

    /// Convert to BGR24 format (OpenCV native)
    ///
    /// # Errors
    ///
    /// Returns an error if conversion is not supported
    pub fn to_bgr24(&self) -> Result<Frame> {
        match self.format {
            FrameFormat::Bgr24 => Ok(Frame::new(
                self.data.clone(),
                self.width,
                self.height,
                FrameFormat::Bgr24,
                self.sequence,
            )),
            FrameFormat::Rgb24 => {
                let mut bgr_data = Vec::with_capacity(self.data.len());
                for pixel in self.data.chunks(3) {
                    bgr_data.push(pixel[2]); // B
                    bgr_data.push(pixel[1]); // G
                    bgr_data.push(pixel[0]); // R
                }
                Ok(Frame::new(
                    bgr_data,
                    self.width,
                    self.height,
                    FrameFormat::Bgr24,
                    self.sequence,
                ))
            }
            FrameFormat::Yuyv => {
                // YUYV to BGR conversion
                let bgr_data = convert_yuyv_to_bgr(&self.data, self.width as usize, self.height as usize);
                Ok(Frame::new(
                    bgr_data,
                    self.width,
                    self.height,
                    FrameFormat::Bgr24,
                    self.sequence,
                ))
            }
            FrameFormat::Gray8 => {
                let mut bgr_data = Vec::with_capacity(self.pixel_count() * 3);
                for &gray in &self.data {
                    bgr_data.push(gray);
                    bgr_data.push(gray);
                    bgr_data.push(gray);
                }
                Ok(Frame::new(
                    bgr_data,
                    self.width,
                    self.height,
                    FrameFormat::Bgr24,
                    self.sequence,
                ))
            }
            _ => Err(CameraError::InvalidFormat {
                expected: "Convertible format".to_string(),
                actual: format!("{:?}", self.format),
            }.into()),
        }
    }

    /// Clone a region of interest from the frame
    ///
    /// # Arguments
    ///
    /// * `x` - Top-left X coordinate
    /// * `y` - Top-left Y coordinate
    /// * `w` - Width of region
    /// * `h` - Height of region
    ///
    /// # Errors
    ///
    /// Returns an error if the region is out of bounds
    pub fn extract_roi(&self, x: u32, y: u32, w: u32, h: u32) -> Result<Frame> {
        if x + w > self.width || y + h > self.height {
            return Err(CameraError::InvalidFormat {
                expected: format!("ROI within {}x{}", self.width, self.height),
                actual: format!("ROI at ({},{}) size {}x{}", x, y, w, h),
            }.into());
        }

        let bpp = self.format.bytes_per_pixel();
        if bpp == 0 {
            return Err(CameraError::InvalidFormat {
                expected: "Non-compressed format".to_string(),
                actual: format!("{:?}", self.format),
            }.into());
        }

        let mut roi_data = Vec::with_capacity((w as usize) * (h as usize) * bpp);
        
        for row in y..(y + h) {
            let start = ((row * self.width + x) as usize) * bpp;
            let end = start + (w as usize) * bpp;
            roi_data.extend_from_slice(&self.data[start..end]);
        }

        Ok(Frame::new(roi_data, w, h, self.format, self.sequence))
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        // Zeroize frame data on drop for security
        self.data.zeroize();
    }
}

impl Clone for Frame {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            width: self.width,
            height: self.height,
            format: self.format,
            timestamp: self.timestamp,
            sequence: self.sequence,
        }
    }
}

/// Convert YUYV (YUV422) to BGR24
fn convert_yuyv_to_bgr(yuyv: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut bgr = Vec::with_capacity(width * height * 3);
    
    for chunk in yuyv.chunks(4) {
        if chunk.len() < 4 {
            break;
        }
        
        let y0 = chunk[0] as f32;
        let u = chunk[1] as f32 - 128.0;
        let y1 = chunk[2] as f32;
        let v = chunk[3] as f32 - 128.0;
        
        // First pixel
        let r0 = (y0 + 1.402 * v).clamp(0.0, 255.0) as u8;
        let g0 = (y0 - 0.344 * u - 0.714 * v).clamp(0.0, 255.0) as u8;
        let b0 = (y0 + 1.772 * u).clamp(0.0, 255.0) as u8;
        bgr.push(b0);
        bgr.push(g0);
        bgr.push(r0);
        
        // Second pixel
        let r1 = (y1 + 1.402 * v).clamp(0.0, 255.0) as u8;
        let g1 = (y1 - 0.344 * u - 0.714 * v).clamp(0.0, 255.0) as u8;
        let b1 = (y1 + 1.772 * u).clamp(0.0, 255.0) as u8;
        bgr.push(b1);
        bgr.push(g1);
        bgr.push(r1);
    }
    
    bgr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_format_bytes_per_pixel() {
        assert_eq!(FrameFormat::Rgb24.bytes_per_pixel(), 3);
        assert_eq!(FrameFormat::Gray8.bytes_per_pixel(), 1);
        assert_eq!(FrameFormat::Mjpeg.bytes_per_pixel(), 0);
    }

    #[test]
    fn test_frame_creation() {
        let data = vec![0u8; 640 * 480 * 3];
        let frame = Frame::new(data, 640, 480, FrameFormat::Rgb24, 1);
        
        assert_eq!(frame.width(), 640);
        assert_eq!(frame.height(), 480);
        assert_eq!(frame.format(), FrameFormat::Rgb24);
        assert_eq!(frame.sequence(), 1);
    }

    #[test]
    fn test_frame_from_bytes_validation() {
        let data = vec![0u8; 100]; // Wrong size for 640x480
        let result = Frame::from_bytes(data, 640, 480, FrameFormat::Rgb24, 1);
        assert!(result.is_err());
        
        let data = vec![0u8; 640 * 480 * 3]; // Correct size
        let result = Frame::from_bytes(data, 640, 480, FrameFormat::Rgb24, 1);
        assert!(result.is_ok());
    }

    #[test]
    fn test_rgb_to_grayscale() {
        // Simple test: all red pixel
        let data = vec![255, 0, 0]; // RGB red
        let frame = Frame::new(data, 1, 1, FrameFormat::Rgb24, 1);
        let gray = frame.to_grayscale().unwrap();
        
        assert_eq!(gray.format(), FrameFormat::Gray8);
        assert_eq!(gray.data().len(), 1);
        // 0.299 * 255 ≈ 76
        assert!((gray.data()[0] as i32 - 76).abs() < 2);
    }

    #[test]
    fn test_extract_roi() {
        let mut data = vec![0u8; 10 * 10];
        // Mark center pixel
        data[55] = 255;
        
        let frame = Frame::new(data, 10, 10, FrameFormat::Gray8, 1);
        let roi = frame.extract_roi(4, 4, 3, 3).unwrap();
        
        assert_eq!(roi.width(), 3);
        assert_eq!(roi.height(), 3);
        // Center pixel should be at (1,1) in ROI = index 4
        assert_eq!(roi.data()[4], 255);
    }

    #[test]
    fn test_extract_roi_bounds_check() {
        let data = vec![0u8; 10 * 10];
        let frame = Frame::new(data, 10, 10, FrameFormat::Gray8, 1);
        
        // Out of bounds
        let result = frame.extract_roi(8, 8, 5, 5);
        assert!(result.is_err());
    }

    #[test]
    fn test_yuyv_conversion() {
        // Simple YUYV test data
        let yuyv = vec![128u8, 128, 128, 128]; // Should produce gray
        let bgr = convert_yuyv_to_bgr(&yuyv, 2, 1);
        
        assert_eq!(bgr.len(), 6); // 2 pixels * 3 bytes
    }
}
