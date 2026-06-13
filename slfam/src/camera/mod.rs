//! # Camera Abstraction Layer
//!
//! This module provides a unified interface for camera access on Linux systems.
//! It supports both RGB and IR cameras with automatic detection and exclusive locking.
//!
//! ## Features
//!
//! - V4L2 device enumeration and detection
//! - IR vs RGB camera identification
//! - Exclusive device locking
//! - Frame capture with timeout
//! - Proper resource cleanup
//!
//! ## Usage
//!
//! ```no_run
//! use slfam::camera::{Camera, CameraType};
//!
//! // Auto-detect and open default RGB camera
//! let camera = Camera::open_default(CameraType::Rgb)?;
//!
//! // Capture a frame
//! let frame = camera.capture_frame()?;
//! ```

mod device;
mod frame;
#[cfg(feature = "dev-mode")]
mod mock;
#[cfg(all(target_os = "linux", feature = "v4l2"))]
mod v4l2;

pub use device::{CameraDevice, CameraInfo, CameraType};
pub use frame::{Frame, FrameFormat};
#[cfg(feature = "dev-mode")]
pub use mock::MockCamera;

use crate::config::CameraConfig;
use crate::error::{CameraError, Result};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Camera trait for unified access to different camera backends
pub trait CameraCapture: Send {
    /// Capture a single frame
    fn capture_frame(&mut self) -> Result<Frame>;
    
    /// Get camera information
    fn info(&self) -> &CameraInfo;
    
    /// Check if camera is still available
    fn is_available(&self) -> bool;
    
    /// Release camera resources
    fn release(&mut self);
}

/// Main camera interface
pub struct Camera {
    /// Camera backend implementation
    inner: Box<dyn CameraCapture>,
    /// Exclusive lock for the device
    _lock: Option<DeviceLock>,
    /// Configuration
    config: CameraConfig,
    /// Last capture timestamp
    last_capture: Option<Instant>,
}

/// Device lock to prevent concurrent access
struct DeviceLock {
    path: PathBuf,
    _lock_file: std::fs::File,
}

impl DeviceLock {
    /// Create an exclusive lock for a device
    fn acquire(path: &PathBuf) -> Result<Self> {
        use std::fs::OpenOptions;
        use std::os::unix::fs::OpenOptionsExt;
        
        let lock_path = PathBuf::from(format!("/var/lock/slfam-{}.lock", 
            path.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "camera".to_string())));
        
        // Ensure lock directory exists
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        
        let lock_file = OpenOptions::new()
            .write(true)
            .create(true)
            .mode(0o644)
            .open(&lock_path)
            .map_err(|e| CameraError::DeviceBusy { 
                path: path.clone() 
            })?;
        
        // Try to acquire exclusive lock (non-blocking)
        use nix::fcntl::{flock, FlockArg};
        use std::os::unix::io::AsRawFd;
        
        flock(lock_file.as_raw_fd(), FlockArg::LockExclusiveNonblock)
            .map_err(|_| CameraError::DeviceBusy { path: path.clone() })?;
        
        Ok(Self {
            path: path.clone(),
            _lock_file: lock_file,
        })
    }
}

impl Drop for DeviceLock {
    fn drop(&mut self) {
        // Lock is automatically released when file is closed
        log::debug!("Released camera lock for {:?}", self.path);
    }
}

impl Camera {
    /// Open a camera with the specified configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Camera configuration
    /// * `camera_type` - Type of camera to open (RGB or IR)
    ///
    /// # Errors
    ///
    /// Returns an error if the camera cannot be opened or locked
    pub fn open(config: &CameraConfig, camera_type: CameraType) -> Result<Self> {
        let device_path = match camera_type {
            CameraType::Rgb => config.device_id.to_path(),
            CameraType::Ir => {
                config.ir_device_id
                    .as_ref()
                    .map(|d| d.to_path())
                    .ok_or(CameraError::IrCameraRequired)?
            }
            CameraType::Unknown => {
                // Default to RGB device for unknown type
                config.device_id.to_path()
            }
        };
        
        Self::open_path(&device_path, config.clone(), camera_type)
    }

    /// Open a camera at the specified path
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the camera device
    /// * `config` - Camera configuration
    /// * `camera_type` - Expected camera type
    ///
    /// # Errors
    ///
    /// Returns an error if the camera cannot be opened
    pub fn open_path(path: &PathBuf, config: CameraConfig, camera_type: CameraType) -> Result<Self> {
        // Check if device exists
        if !path.exists() {
            return Err(CameraError::DeviceNotFound { path: path.clone() }.into());
        }
        
        // Acquire exclusive lock
        let lock = DeviceLock::acquire(path)?;
        
        // Create V4L2 backend
        #[cfg(all(target_os = "linux", feature = "v4l2"))]
        let backend = v4l2::V4l2Camera::open(path, &config, camera_type)?;
        
        #[cfg(not(all(target_os = "linux", feature = "v4l2")))]
        return Err(CameraError::NotSupported("V4L2 support not compiled".to_string()).into());
        
        #[cfg(all(target_os = "linux", feature = "v4l2"))]
        Ok(Self {
            inner: Box::new(backend),
            _lock: Some(lock),
            config,
            last_capture: None,
        })
    }

    /// Open the default camera for the specified type
    ///
    /// # Arguments
    ///
    /// * `camera_type` - Type of camera to open
    ///
    /// # Errors
    ///
    /// Returns an error if no suitable camera is found
    pub fn open_default(camera_type: CameraType) -> Result<Self> {
        let config = CameraConfig::default();
        Self::open(&config, camera_type)
    }

    /// Capture a frame with timeout
    ///
    /// # Errors
    ///
    /// Returns an error if capture fails or times out
    pub fn capture_frame(&mut self) -> Result<Frame> {
        let timeout = Duration::from_millis(self.config.capture_timeout_ms);
        let start = Instant::now();
        
        loop {
            match self.inner.capture_frame() {
                Ok(frame) => {
                    self.last_capture = Some(Instant::now());
                    return Ok(frame);
                }
                Err(e) if start.elapsed() < timeout => {
                    // Retry if within timeout
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(_) => {
                    return Err(CameraError::CaptureTimeout {
                        timeout_ms: self.config.capture_timeout_ms,
                    }.into());
                }
            }
        }
    }

    /// Capture multiple frames
    ///
    /// # Arguments
    ///
    /// * `count` - Number of frames to capture
    /// * `interval` - Interval between frames
    ///
    /// # Errors
    ///
    /// Returns an error if any capture fails
    pub fn capture_frames(&mut self, count: usize, interval: Duration) -> Result<Vec<Frame>> {
        let mut frames = Vec::with_capacity(count);
        
        for i in 0..count {
            let frame = self.capture_frame()?;
            frames.push(frame);
            
            if i < count - 1 {
                std::thread::sleep(interval);
            }
        }
        
        Ok(frames)
    }

    /// Get camera information
    #[must_use]
    pub fn info(&self) -> &CameraInfo {
        self.inner.info()
    }

    /// Check if this is an IR camera
    #[must_use]
    pub fn is_ir(&self) -> bool {
        self.inner.info().camera_type == CameraType::Ir
    }

    /// Start streaming (no-op for some backends, required for others)
    pub fn start_streaming(&mut self) -> Result<()> {
        // Streaming is started automatically on first capture for most backends
        Ok(())
    }

    /// Stop streaming
    pub fn stop_streaming(&mut self) -> Result<()> {
        // Streaming is stopped automatically when camera is dropped
        Ok(())
    }

    /// Get device path
    #[must_use]
    pub fn device_path(&self) -> String {
        self.inner.info().path.to_string_lossy().to_string()
    }

    /// Check if camera is still available
    #[must_use]
    pub fn is_available(&self) -> bool {
        self.inner.is_available()
    }

    /// Get time since last capture
    #[must_use]
    pub fn time_since_last_capture(&self) -> Option<Duration> {
        self.last_capture.map(|t| t.elapsed())
    }
}

impl Drop for Camera {
    fn drop(&mut self) {
        self.inner.release();
    }
}

/// Enumerate all available cameras on the system
///
/// # Returns
///
/// A vector of camera information for all detected cameras
pub fn enumerate_cameras() -> Vec<CameraInfo> {
    let mut cameras = Vec::new();
    
    #[cfg(all(target_os = "linux", feature = "v4l2"))]
    {
        // Scan /dev/video* devices
        for entry in std::fs::read_dir("/dev").unwrap_or_else(|_| std::fs::read_dir(".").unwrap()) {
            if let Ok(entry) = entry {
                let path = entry.path();
                if let Some(name) = path.file_name() {
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with("video") {
                        if let Ok(info) = v4l2::query_camera_info(&path) {
                            cameras.push(info);
                        }
                    }
                }
            }
        }
    }
    
    cameras
}

/// Find an IR camera from available devices
///
/// # Returns
///
/// Camera information for the first detected IR camera, if any
pub fn find_ir_camera() -> Option<CameraInfo> {
    enumerate_cameras()
        .into_iter()
        .find(|c| c.camera_type == CameraType::Ir)
}

/// Find an RGB camera from available devices
///
/// # Returns
///
/// Camera information for the first detected RGB camera, if any
pub fn find_rgb_camera() -> Option<CameraInfo> {
    enumerate_cameras()
        .into_iter()
        .find(|c| c.camera_type == CameraType::Rgb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_type_enum() {
        assert!(CameraType::Rgb != CameraType::Ir);
    }

    #[test]
    fn test_camera_enumeration() {
        // This test just verifies the enumeration doesn't panic
        let cameras = enumerate_cameras();
        // Note: might be empty on CI systems without cameras
        println!("Found {} cameras", cameras.len());
    }

    #[test]
    fn test_device_lock_path_generation() {
        let path = PathBuf::from("/dev/video0");
        // Just verify we can construct the lock path
        let lock_path = format!("/var/lock/slfam-{}.lock", 
            path.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "camera".to_string()));
        assert!(lock_path.contains("video0"));
    }
}
