//! Mock camera for testing and development
//!
//! This module provides a mock camera implementation that can be used
//! for testing without requiring actual camera hardware.

use super::device::{CameraCapabilities, CameraInfo, CameraType};
use super::frame::{Frame, FrameFormat};
use super::CameraCapture;
use crate::error::Result;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Mock camera for testing
pub struct MockCamera {
    /// Camera info
    info: CameraInfo,
    /// Configured width
    width: u32,
    /// Configured height
    height: u32,
    /// Frame format
    format: FrameFormat,
    /// Frame sequence counter
    sequence: AtomicU64,
    /// Pre-loaded test frames
    frames: Vec<Vec<u8>>,
    /// Current frame index
    current_frame: usize,
    /// Is camera available
    available: bool,
}

impl MockCamera {
    /// Create a new mock camera with default settings
    pub fn new() -> Self {
        Self::with_resolution(640, 480)
    }

    /// Create a mock camera with specific resolution
    pub fn with_resolution(width: u32, height: u32) -> Self {
        let info = CameraInfo {
            path: PathBuf::from("/dev/mock_video0"),
            name: "Mock Camera".to_string(),
            camera_type: CameraType::Rgb,
            index: 0,
            resolutions: vec![(320, 240), (640, 480), (1280, 720)],
            frame_rates: vec![15, 30, 60],
            driver: "mock".to_string(),
            bus_info: "mock:0".to_string(),
            card: "Mock Camera".to_string(),
            capabilities: CameraCapabilities {
                video_capture: true,
                streaming: true,
                read_write: true,
                ir_capable: false,
                auto_focus: false,
                exposure_control: false,
            },
        };

        Self {
            info,
            width,
            height,
            format: FrameFormat::Rgb24,
            sequence: AtomicU64::new(0),
            frames: Vec::new(),
            current_frame: 0,
            available: true,
        }
    }

    /// Create a mock IR camera
    pub fn new_ir() -> Self {
        let mut camera = Self::with_resolution(640, 480);
        camera.info.name = "Mock IR Camera".to_string();
        camera.info.camera_type = CameraType::Ir;
        camera.info.capabilities.ir_capable = true;
        camera.format = FrameFormat::Gray8;
        camera
    }

    /// Add a test frame to the camera
    pub fn add_frame(&mut self, data: Vec<u8>) {
        self.frames.push(data);
    }

    /// Generate a synthetic test frame with a face-like pattern
    pub fn generate_test_frame(&self) -> Vec<u8> {
        let pixel_count = (self.width * self.height) as usize;
        let mut data = match self.format {
            FrameFormat::Rgb24 => vec![128u8; pixel_count * 3],
            FrameFormat::Gray8 => vec![128u8; pixel_count],
            _ => vec![128u8; pixel_count * 3],
        };

        // Draw a simple oval "face" in the center
        let cx = self.width as i32 / 2;
        let cy = self.height as i32 / 2;
        let rx = self.width as i32 / 4;
        let ry = self.height as i32 / 3;

        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                let dx = (x - cx) as f32 / rx as f32;
                let dy = (y - cy) as f32 / ry as f32;
                
                // Check if inside face oval
                if dx * dx + dy * dy < 1.0 {
                    let idx = (y as u32 * self.width + x as u32) as usize;
                    match self.format {
                        FrameFormat::Rgb24 => {
                            let idx3 = idx * 3;
                            // Skin tone color (light brown)
                            data[idx3] = 220;     // R
                            data[idx3 + 1] = 180; // G
                            data[idx3 + 2] = 160; // B
                        }
                        FrameFormat::Gray8 => {
                            data[idx] = 180;
                        }
                        _ => {}
                    }
                }

                // Draw "eyes" as darker spots
                let eye_y = cy - ry / 3;
                let left_eye_x = cx - rx / 3;
                let right_eye_x = cx + rx / 3;
                let eye_r = rx / 8;

                let dist_left = ((x - left_eye_x).pow(2) + (y - eye_y).pow(2)) as f32;
                let dist_right = ((x - right_eye_x).pow(2) + (y - eye_y).pow(2)) as f32;
                let eye_r_sq = (eye_r * eye_r) as f32;

                if dist_left < eye_r_sq || dist_right < eye_r_sq {
                    let idx = (y as u32 * self.width + x as u32) as usize;
                    match self.format {
                        FrameFormat::Rgb24 => {
                            let idx3 = idx * 3;
                            data[idx3] = 50;
                            data[idx3 + 1] = 50;
                            data[idx3 + 2] = 50;
                        }
                        FrameFormat::Gray8 => {
                            data[idx] = 50;
                        }
                        _ => {}
                    }
                }
            }
        }

        data
    }

    /// Set camera availability (for testing failure scenarios)
    pub fn set_available(&mut self, available: bool) {
        self.available = available;
    }

    /// Load frame from image file (for testing with real images)
    #[cfg(feature = "dev-mode")]
    pub fn load_frame_from_file<P: AsRef<std::path::Path>>(&mut self, path: P) -> Result<()> {
        use std::fs::File;
        use std::io::Read;

        let mut file = File::open(path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;

        // Note: In a real implementation, we'd decode the image format
        // For now, assume it's raw RGB data
        self.frames.push(data);
        Ok(())
    }
}

impl Default for MockCamera {
    fn default() -> Self {
        Self::new()
    }
}

impl CameraCapture for MockCamera {
    fn capture_frame(&mut self) -> Result<Frame> {
        if !self.available {
            return Err(crate::error::CameraError::CaptureFailed(
                "Mock camera unavailable".to_string()
            ).into());
        }

        let data = if self.frames.is_empty() {
            self.generate_test_frame()
        } else {
            let frame = self.frames[self.current_frame].clone();
            self.current_frame = (self.current_frame + 1) % self.frames.len();
            frame
        };

        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);
        Ok(Frame::new(data, self.width, self.height, self.format, seq))
    }

    fn info(&self) -> &CameraInfo {
        &self.info
    }

    fn is_available(&self) -> bool {
        self.available
    }

    fn release(&mut self) {
        self.available = false;
    }
}

/// Frame generator for testing liveness detection
pub struct LivenessTestFrames;

impl LivenessTestFrames {
    /// Generate frames simulating a blink
    pub fn generate_blink_sequence(width: u32, height: u32, frame_count: usize) -> Vec<Vec<u8>> {
        let mut frames = Vec::with_capacity(frame_count);
        
        for i in 0..frame_count {
            let mut camera = MockCamera::with_resolution(width, height);
            let mut frame = camera.generate_test_frame();
            
            // Modify eye region based on frame number to simulate blink
            // Middle frames have "closed" eyes
            let is_blinking = i >= frame_count / 3 && i < 2 * frame_count / 3;
            
            if is_blinking {
                // Close eyes by making eye region match skin tone
                let cx = width as i32 / 2;
                let cy = height as i32 / 2;
                let rx = width as i32 / 4;
                let ry = height as i32 / 3;
                let eye_y = cy - ry / 3;
                let left_eye_x = cx - rx / 3;
                let right_eye_x = cx + rx / 3;
                let eye_rx = rx / 6;
                let eye_ry = rx / 12; // Thin slit when closed

                for y in (eye_y - eye_ry)..(eye_y + eye_ry) {
                    for x in (left_eye_x - eye_rx)..(right_eye_x + eye_rx) {
                        if x >= 0 && x < width as i32 && y >= 0 && y < height as i32 {
                            let idx = ((y as u32 * width + x as u32) * 3) as usize;
                            if idx + 2 < frame.len() {
                                // Skin tone
                                frame[idx] = 220;
                                frame[idx + 1] = 180;
                                frame[idx + 2] = 160;
                            }
                        }
                    }
                }
            }
            
            frames.push(frame);
        }
        
        frames
    }

    /// Generate frames simulating a static photo (no movement)
    pub fn generate_static_frames(width: u32, height: u32, frame_count: usize) -> Vec<Vec<u8>> {
        let camera = MockCamera::with_resolution(width, height);
        let frame = camera.generate_test_frame();
        
        // Return identical frames
        vec![frame; frame_count]
    }

    /// Generate frames with slight natural movement
    pub fn generate_natural_movement(width: u32, height: u32, frame_count: usize) -> Vec<Vec<u8>> {
        let mut frames = Vec::with_capacity(frame_count);
        
        for i in 0..frame_count {
            // Add slight offset to simulate micro-movements
            let offset_x = ((i as f32 * 0.3).sin() * 2.0) as i32;
            let offset_y = ((i as f32 * 0.5).cos() * 1.5) as i32;
            
            let camera = MockCamera::with_resolution(width, height);
            let base_frame = camera.generate_test_frame();
            
            // Shift frame slightly (simplified - just adds noise in practice)
            let mut shifted = base_frame.clone();
            for (idx, pixel) in shifted.iter_mut().enumerate() {
                *pixel = pixel.saturating_add((offset_x.abs() + offset_y.abs()) as u8 % 10);
            }
            
            frames.push(shifted);
        }
        
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_camera_creation() {
        let camera = MockCamera::new();
        assert_eq!(camera.info().name, "Mock Camera");
        assert_eq!(camera.info().camera_type, CameraType::Rgb);
    }

    #[test]
    fn test_mock_ir_camera() {
        let camera = MockCamera::new_ir();
        assert!(camera.info().is_ir());
        assert_eq!(camera.format, FrameFormat::Gray8);
    }

    #[test]
    fn test_capture_frame() {
        let mut camera = MockCamera::new();
        let frame = camera.capture_frame().unwrap();
        
        assert_eq!(frame.width(), 640);
        assert_eq!(frame.height(), 480);
        assert_eq!(frame.sequence(), 0);

        // Capture another frame
        let frame2 = camera.capture_frame().unwrap();
        assert_eq!(frame2.sequence(), 1);
    }

    #[test]
    fn test_custom_frames() {
        let mut camera = MockCamera::with_resolution(10, 10);
        let test_data = vec![255u8; 10 * 10 * 3];
        camera.add_frame(test_data.clone());
        
        let frame = camera.capture_frame().unwrap();
        assert_eq!(frame.data().len(), test_data.len());
    }

    #[test]
    fn test_unavailable_camera() {
        let mut camera = MockCamera::new();
        camera.set_available(false);
        
        let result = camera.capture_frame();
        assert!(result.is_err());
    }

    #[test]
    fn test_blink_sequence_generation() {
        let frames = LivenessTestFrames::generate_blink_sequence(320, 240, 10);
        assert_eq!(frames.len(), 10);
        
        // Check frames have correct size
        let expected_size = 320 * 240 * 3;
        for frame in &frames {
            assert_eq!(frame.len(), expected_size);
        }
    }

    #[test]
    fn test_static_frames_identical() {
        let frames = LivenessTestFrames::generate_static_frames(100, 100, 5);
        
        // All frames should be identical
        for i in 1..frames.len() {
            assert_eq!(frames[0], frames[i]);
        }
    }

    #[test]
    fn test_natural_movement_different() {
        let frames = LivenessTestFrames::generate_natural_movement(100, 100, 5);
        
        // Frames should have some variation
        let mut all_same = true;
        for i in 1..frames.len() {
            if frames[0] != frames[i] {
                all_same = false;
                break;
            }
        }
        assert!(!all_same, "Natural movement frames should have variation");
    }
}
