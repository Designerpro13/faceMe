//! Camera device types and information

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Type of camera
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CameraType {
    /// Standard RGB camera
    Rgb,
    /// Infrared camera
    Ir,
    /// Unknown camera type
    Unknown,
}

impl std::fmt::Display for CameraType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CameraType::Rgb => write!(f, "RGB"),
            CameraType::Ir => write!(f, "IR"),
            CameraType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Camera device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraInfo {
    /// Device path (e.g., /dev/video0)
    pub path: PathBuf,
    /// Device name from driver
    pub name: String,
    /// Camera type (RGB/IR)
    pub camera_type: CameraType,
    /// Device index
    pub index: u32,
    /// Supported resolutions
    pub resolutions: Vec<(u32, u32)>,
    /// Supported frame rates
    pub frame_rates: Vec<u32>,
    /// Driver name
    pub driver: String,
    /// Bus information
    pub bus_info: String,
    /// Card name
    pub card: String,
    /// Capabilities flags
    pub capabilities: CameraCapabilities,
}

/// Camera capabilities flags
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CameraCapabilities {
    /// Supports video capture
    pub video_capture: bool,
    /// Supports streaming
    pub streaming: bool,
    /// Supports read/write
    pub read_write: bool,
    /// Is an IR camera
    pub ir_capable: bool,
    /// Supports auto-focus
    pub auto_focus: bool,
    /// Supports exposure control
    pub exposure_control: bool,
}

impl CameraInfo {
    /// Create a new camera info with basic details
    pub fn new(path: PathBuf, name: String, camera_type: CameraType, index: u32) -> Self {
        Self {
            path,
            name,
            camera_type,
            index,
            resolutions: Vec::new(),
            frame_rates: Vec::new(),
            driver: String::new(),
            bus_info: String::new(),
            card: String::new(),
            capabilities: CameraCapabilities::default(),
        }
    }

    /// Check if this is an IR camera
    #[must_use]
    pub fn is_ir(&self) -> bool {
        self.camera_type == CameraType::Ir || self.capabilities.ir_capable
    }

    /// Check if this is an RGB camera
    #[must_use]
    pub fn is_rgb(&self) -> bool {
        self.camera_type == CameraType::Rgb
    }

    /// Get the best resolution for face recognition
    #[must_use]
    pub fn best_resolution(&self) -> Option<(u32, u32)> {
        // Prefer 640x480 for face recognition, otherwise closest match
        if self.resolutions.contains(&(640, 480)) {
            return Some((640, 480));
        }
        
        // Find closest to 640x480
        self.resolutions
            .iter()
            .min_by_key(|(w, h)| {
                let diff_w = (*w as i32 - 640).abs();
                let diff_h = (*h as i32 - 480).abs();
                diff_w + diff_h
            })
            .copied()
    }
}

/// Camera device identifier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CameraDevice {
    /// Device by index
    Index(u32),
    /// Device by path
    Path(String),
    /// Auto-detect
    Auto,
}

impl Default for CameraDevice {
    fn default() -> Self {
        CameraDevice::Index(0)
    }
}

impl CameraDevice {
    /// Convert to device path
    #[must_use]
    pub fn to_path(&self) -> PathBuf {
        match self {
            CameraDevice::Index(idx) => PathBuf::from(format!("/dev/video{}", idx)),
            CameraDevice::Path(path) => PathBuf::from(path),
            CameraDevice::Auto => PathBuf::from("/dev/video0"),
        }
    }

    /// Create from device path
    #[must_use]
    pub fn from_path<S: Into<String>>(path: S) -> Self {
        CameraDevice::Path(path.into())
    }

    /// Create from device index
    #[must_use]
    pub fn from_index(index: u32) -> Self {
        CameraDevice::Index(index)
    }
}

/// Heuristics for detecting IR cameras
pub mod ir_detection {
    use super::*;

    /// Common IR camera name patterns
    const IR_NAME_PATTERNS: &[&str] = &[
        "ir", "infrared", "depth", "3d", "tof", "time-of-flight",
        "realsense", "kinect", "azure", "primesense",
    ];

    /// Check if camera name suggests IR capability
    #[must_use]
    pub fn is_likely_ir_camera(info: &CameraInfo) -> bool {
        let name_lower = info.name.to_lowercase();
        let card_lower = info.card.to_lowercase();
        
        for pattern in IR_NAME_PATTERNS {
            if name_lower.contains(pattern) || card_lower.contains(pattern) {
                return true;
            }
        }
        
        // Check driver hints
        if info.driver.to_lowercase().contains("uvcvideo") {
            // Many IR cameras use uvcvideo driver, need additional checks
            // Check for grayscale-only capabilities in supported formats
        }
        
        info.capabilities.ir_capable
    }

    /// Determine camera type from device information
    #[must_use]
    pub fn detect_camera_type(info: &CameraInfo) -> CameraType {
        if is_likely_ir_camera(info) {
            CameraType::Ir
        } else {
            CameraType::Rgb
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_type_display() {
        assert_eq!(format!("{}", CameraType::Rgb), "RGB");
        assert_eq!(format!("{}", CameraType::Ir), "IR");
    }

    #[test]
    fn test_camera_device_to_path() {
        let device = CameraDevice::Index(0);
        assert_eq!(device.to_path(), PathBuf::from("/dev/video0"));

        let device = CameraDevice::Path("/dev/video2".to_string());
        assert_eq!(device.to_path(), PathBuf::from("/dev/video2"));
    }

    #[test]
    fn test_camera_info_best_resolution() {
        let mut info = CameraInfo::new(
            PathBuf::from("/dev/video0"),
            "Test Camera".to_string(),
            CameraType::Rgb,
            0,
        );
        
        info.resolutions = vec![(320, 240), (640, 480), (1280, 720)];
        assert_eq!(info.best_resolution(), Some((640, 480)));
        
        info.resolutions = vec![(320, 240), (1280, 720)];
        assert_eq!(info.best_resolution(), Some((320, 240)));
    }

    #[test]
    fn test_ir_detection() {
        let mut info = CameraInfo::new(
            PathBuf::from("/dev/video0"),
            "Intel RealSense IR".to_string(),
            CameraType::Unknown,
            0,
        );
        
        assert!(ir_detection::is_likely_ir_camera(&info));
        assert_eq!(ir_detection::detect_camera_type(&info), CameraType::Ir);
        
        info.name = "Logitech HD Webcam C920".to_string();
        assert!(!ir_detection::is_likely_ir_camera(&info));
        assert_eq!(ir_detection::detect_camera_type(&info), CameraType::Rgb);
    }
}
