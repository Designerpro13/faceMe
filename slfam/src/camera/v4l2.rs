//! V4L2 camera backend for Linux

use super::device::{CameraCapabilities, CameraInfo, CameraType, ir_detection};
use super::frame::{Frame, FrameFormat};
use super::CameraCapture;
use crate::config::CameraConfig;
use crate::error::{CameraError, Result};
use std::fs::{File, OpenOptions};
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// V4L2 format codes
mod format_codes {
    pub const V4L2_PIX_FMT_YUYV: u32 = 0x56595559;
    pub const V4L2_PIX_FMT_MJPEG: u32 = 0x47504A4D;
    pub const V4L2_PIX_FMT_RGB24: u32 = 0x33424752;
    pub const V4L2_PIX_FMT_BGR24: u32 = 0x33524742;
    pub const V4L2_PIX_FMT_GREY: u32 = 0x59455247;
    pub const V4L2_PIX_FMT_Y16: u32 = 0x20363159;
}

/// V4L2 capability flags
mod cap_flags {
    pub const V4L2_CAP_VIDEO_CAPTURE: u32 = 0x00000001;
    pub const V4L2_CAP_STREAMING: u32 = 0x04000000;
    pub const V4L2_CAP_READWRITE: u32 = 0x01000000;
}

/// V4L2 buffer type
const V4L2_BUF_TYPE_VIDEO_CAPTURE: u32 = 1;

/// V4L2 memory type
const V4L2_MEMORY_MMAP: u32 = 1;

/// V4L2 camera backend
pub struct V4l2Camera {
    /// Device file
    file: File,
    /// Device path
    path: PathBuf,
    /// Camera information
    info: CameraInfo,
    /// Frame sequence counter
    sequence: AtomicU64,
    /// Configured width
    width: u32,
    /// Configured height
    height: u32,
    /// Configured format
    format: FrameFormat,
    /// Whether streaming is active
    streaming: bool,
    /// Memory-mapped buffers
    buffers: Vec<MappedBuffer>,
}

/// Memory-mapped buffer
struct MappedBuffer {
    ptr: *mut u8,
    length: usize,
}

unsafe impl Send for MappedBuffer {}

impl Drop for MappedBuffer {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                libc::munmap(self.ptr as *mut libc::c_void, self.length);
            }
        }
    }
}

impl V4l2Camera {
    /// Open a V4L2 camera device
    pub fn open(path: &Path, config: &CameraConfig, expected_type: CameraType) -> Result<Self> {
        // Open device with read/write permissions
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    CameraError::PermissionDenied { path: path.to_path_buf() }
                } else if e.kind() == std::io::ErrorKind::NotFound {
                    CameraError::DeviceNotFound { path: path.to_path_buf() }
                } else {
                    CameraError::InitializationFailed(e.to_string())
                }
            })?;

        let fd = file.as_raw_fd();

        // Query device capabilities
        let mut info = query_capabilities(fd, path)?;

        // Determine camera type
        let detected_type = ir_detection::detect_camera_type(&info);
        info.camera_type = detected_type;

        // Log warning if expected type doesn't match
        if expected_type != CameraType::Unknown && expected_type != detected_type {
            log::warn!(
                "Camera at {:?} detected as {:?} but expected {:?}",
                path, detected_type, expected_type
            );
        }

        // Set format
        let (width, height, format) = set_format(fd, config.frame_width, config.frame_height)?;

        // Query supported resolutions
        info.resolutions = query_resolutions(fd);
        
        let mut camera = Self {
            file,
            path: path.to_path_buf(),
            info,
            sequence: AtomicU64::new(0),
            width,
            height,
            format,
            streaming: false,
            buffers: Vec::new(),
        };

        // Initialize memory-mapped buffers
        camera.init_mmap(4)?;

        // Start streaming
        camera.start_streaming()?;

        Ok(camera)
    }

    /// Initialize memory-mapped buffers
    fn init_mmap(&mut self, buffer_count: u32) -> Result<()> {
        let fd = self.file.as_raw_fd();

        // Request buffers
        let mut reqbuf = v4l2_requestbuffers {
            count: buffer_count,
            type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
            memory: V4L2_MEMORY_MMAP,
            ..Default::default()
        };

        unsafe {
            if ioctl_reqbufs(fd, &mut reqbuf) < 0 {
                return Err(CameraError::InitializationFailed(
                    "Failed to request buffers".to_string()
                ).into());
            }
        }

        // Map buffers
        for i in 0..reqbuf.count {
            let mut buf = v4l2_buffer {
                index: i,
                type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
                memory: V4L2_MEMORY_MMAP,
                ..Default::default()
            };

            unsafe {
                if ioctl_querybuf(fd, &mut buf) < 0 {
                    return Err(CameraError::InitializationFailed(
                        format!("Failed to query buffer {}", i)
                    ).into());
                }

                let ptr = libc::mmap(
                    std::ptr::null_mut(),
                    buf.length as usize,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    fd,
                    buf.m.offset as i64,
                );

                if ptr == libc::MAP_FAILED {
                    return Err(CameraError::InitializationFailed(
                        "Failed to mmap buffer".to_string()
                    ).into());
                }

                self.buffers.push(MappedBuffer {
                    ptr: ptr as *mut u8,
                    length: buf.length as usize,
                });

                // Queue buffer
                if ioctl_qbuf(fd, &mut buf) < 0 {
                    return Err(CameraError::InitializationFailed(
                        format!("Failed to queue buffer {}", i)
                    ).into());
                }
            }
        }

        Ok(())
    }

    /// Start video streaming
    fn start_streaming(&mut self) -> Result<()> {
        let fd = self.file.as_raw_fd();
        let buf_type = V4L2_BUF_TYPE_VIDEO_CAPTURE;

        unsafe {
            if ioctl_streamon(fd, &buf_type) < 0 {
                return Err(CameraError::InitializationFailed(
                    "Failed to start streaming".to_string()
                ).into());
            }
        }

        self.streaming = true;
        Ok(())
    }

    /// Stop video streaming
    fn stop_streaming(&mut self) {
        if !self.streaming {
            return;
        }

        let fd = self.file.as_raw_fd();
        let buf_type = V4L2_BUF_TYPE_VIDEO_CAPTURE;

        unsafe {
            ioctl_streamoff(fd, &buf_type);
        }

        self.streaming = false;
    }
}

impl CameraCapture for V4l2Camera {
    fn capture_frame(&mut self) -> Result<Frame> {
        if !self.streaming {
            return Err(CameraError::CaptureFailed("Camera not streaming".to_string()).into());
        }

        let fd = self.file.as_raw_fd();

        // Dequeue buffer
        let mut buf = v4l2_buffer {
            type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
            memory: V4L2_MEMORY_MMAP,
            ..Default::default()
        };

        unsafe {
            // Wait for data with timeout
            let mut fds = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };

            let ret = libc::poll(&mut fds, 1, 5000); // 5 second timeout
            if ret <= 0 {
                return Err(CameraError::CaptureTimeout { timeout_ms: 5000 }.into());
            }

            if ioctl_dqbuf(fd, &mut buf) < 0 {
                return Err(CameraError::CaptureFailed("Failed to dequeue buffer".to_string()).into());
            }
        }

        // Copy frame data
        let buffer = &self.buffers[buf.index as usize];
        let data = unsafe {
            std::slice::from_raw_parts(buffer.ptr, buf.bytesused as usize).to_vec()
        };

        // Re-queue buffer
        unsafe {
            if ioctl_qbuf(fd, &mut buf) < 0 {
                log::warn!("Failed to re-queue buffer");
            }
        }

        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);
        Frame::from_bytes(data, self.width, self.height, self.format, seq)
    }

    fn info(&self) -> &CameraInfo {
        &self.info
    }

    fn is_available(&self) -> bool {
        self.path.exists() && self.streaming
    }

    fn release(&mut self) {
        self.stop_streaming();
        self.buffers.clear();
    }
}

impl Drop for V4l2Camera {
    fn drop(&mut self) {
        self.release();
    }
}

/// Query camera information from path
pub fn query_camera_info(path: &Path) -> Result<CameraInfo> {
    let file = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|_| CameraError::DeviceNotFound { path: path.to_path_buf() })?;

    let fd = file.as_raw_fd();
    query_capabilities(fd, path)
}

/// Query device capabilities
fn query_capabilities(fd: RawFd, path: &Path) -> Result<CameraInfo> {
    let mut cap = v4l2_capability::default();

    unsafe {
        if ioctl_querycap(fd, &mut cap) < 0 {
            return Err(CameraError::InitializationFailed(
                "Failed to query capabilities".to_string()
            ).into());
        }
    }

    // Extract device index from path
    let index = path
        .file_name()
        .and_then(|s| s.to_str())
        .and_then(|s| s.strip_prefix("video"))
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    let capabilities = CameraCapabilities {
        video_capture: (cap.capabilities & cap_flags::V4L2_CAP_VIDEO_CAPTURE) != 0,
        streaming: (cap.capabilities & cap_flags::V4L2_CAP_STREAMING) != 0,
        read_write: (cap.capabilities & cap_flags::V4L2_CAP_READWRITE) != 0,
        ir_capable: false, // Will be updated by heuristics
        auto_focus: false,
        exposure_control: false,
    };

    let driver = std::str::from_utf8(&cap.driver)
        .unwrap_or("")
        .trim_end_matches('\0')
        .to_string();
    let card = std::str::from_utf8(&cap.card)
        .unwrap_or("")
        .trim_end_matches('\0')
        .to_string();
    let bus_info = std::str::from_utf8(&cap.bus_info)
        .unwrap_or("")
        .trim_end_matches('\0')
        .to_string();

    Ok(CameraInfo {
        path: path.to_path_buf(),
        name: card.clone(),
        camera_type: CameraType::Unknown,
        index,
        resolutions: Vec::new(),
        frame_rates: Vec::new(),
        driver,
        bus_info,
        card,
        capabilities,
    })
}

/// Set video format and return actual dimensions
fn set_format(fd: RawFd, width: u32, height: u32) -> Result<(u32, u32, FrameFormat)> {
    let mut fmt = v4l2_format {
        type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
        fmt: v4l2_format_union {
            pix: v4l2_pix_format {
                width,
                height,
                pixelformat: format_codes::V4L2_PIX_FMT_YUYV,
                field: 1, // V4L2_FIELD_NONE
                ..Default::default()
            },
        },
    };

    unsafe {
        // Try YUYV first
        if ioctl_s_fmt(fd, &mut fmt) < 0 {
            // Try MJPEG as fallback
            fmt.fmt.pix.pixelformat = format_codes::V4L2_PIX_FMT_MJPEG;
            if ioctl_s_fmt(fd, &mut fmt) < 0 {
                // Try RGB24
                fmt.fmt.pix.pixelformat = format_codes::V4L2_PIX_FMT_RGB24;
                if ioctl_s_fmt(fd, &mut fmt) < 0 {
                    return Err(CameraError::InitializationFailed(
                        "No supported format found".to_string()
                    ).into());
                }
            }
        }

        // Get actual format
        if ioctl_g_fmt(fd, &mut fmt) < 0 {
            return Err(CameraError::InitializationFailed(
                "Failed to get format".to_string()
            ).into());
        }

        let pix = fmt.fmt.pix;
        let format = match pix.pixelformat {
            format_codes::V4L2_PIX_FMT_YUYV => FrameFormat::Yuyv,
            format_codes::V4L2_PIX_FMT_MJPEG => FrameFormat::Mjpeg,
            format_codes::V4L2_PIX_FMT_RGB24 => FrameFormat::Rgb24,
            format_codes::V4L2_PIX_FMT_BGR24 => FrameFormat::Bgr24,
            format_codes::V4L2_PIX_FMT_GREY => FrameFormat::Gray8,
            format_codes::V4L2_PIX_FMT_Y16 => FrameFormat::Gray16,
            _ => FrameFormat::Yuyv,
        };

        Ok((pix.width, pix.height, format))
    }
}

/// Query supported resolutions
fn query_resolutions(fd: RawFd) -> Vec<(u32, u32)> {
    let mut resolutions = Vec::new();
    
    // Common resolutions to try
    let common = [
        (320, 240),
        (640, 480),
        (800, 600),
        (1024, 768),
        (1280, 720),
        (1920, 1080),
    ];

    for (w, h) in common {
        let mut fmt = v4l2_format {
            type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
            fmt: v4l2_format_union {
                pix: v4l2_pix_format {
                    width: w,
                    height: h,
                    pixelformat: format_codes::V4L2_PIX_FMT_YUYV,
                    ..Default::default()
                },
            },
        };

        unsafe {
            // Try to set format and see what we get
            if ioctl_try_fmt(fd, &mut fmt) >= 0 {
                let actual_w = fmt.fmt.pix.width;
                let actual_h = fmt.fmt.pix.height;
                if !resolutions.contains(&(actual_w, actual_h)) {
                    resolutions.push((actual_w, actual_h));
                }
            }
        }
    }

    resolutions.sort();
    resolutions
}

// V4L2 structures (simplified versions)
#[repr(C)]
#[derive(Default)]
struct v4l2_capability {
    driver: [u8; 16],
    card: [u8; 32],
    bus_info: [u8; 32],
    version: u32,
    capabilities: u32,
    device_caps: u32,
    reserved: [u32; 3],
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct v4l2_pix_format {
    width: u32,
    height: u32,
    pixelformat: u32,
    field: u32,
    bytesperline: u32,
    sizeimage: u32,
    colorspace: u32,
    priv_: u32,
    flags: u32,
    hsv_enc: u32,
    quantization: u32,
    xfer_func: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
union v4l2_format_union {
    pix: v4l2_pix_format,
    raw: [u8; 200],
}

impl Default for v4l2_format_union {
    fn default() -> Self {
        Self { raw: [0; 200] }
    }
}

#[repr(C)]
#[derive(Default)]
struct v4l2_format {
    type_: u32,
    fmt: v4l2_format_union,
}

#[repr(C)]
#[derive(Default)]
struct v4l2_requestbuffers {
    count: u32,
    type_: u32,
    memory: u32,
    capabilities: u32,
    flags: u8,
    reserved: [u8; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
union v4l2_buffer_m {
    offset: u32,
    userptr: u64,
    planes: *mut u8,
    fd: i32,
}

impl Default for v4l2_buffer_m {
    fn default() -> Self {
        Self { offset: 0 }
    }
}

#[repr(C)]
#[derive(Default)]
struct v4l2_timecode {
    type_: u32,
    flags: u32,
    frames: u8,
    seconds: u8,
    minutes: u8,
    hours: u8,
    userbits: [u8; 4],
}

#[repr(C)]
#[derive(Default)]
struct v4l2_buffer {
    index: u32,
    type_: u32,
    bytesused: u32,
    flags: u32,
    field: u32,
    timestamp: libc::timeval,
    timecode: v4l2_timecode,
    sequence: u32,
    memory: u32,
    m: v4l2_buffer_m,
    length: u32,
    reserved2: u32,
    request_fd_or_reserved: i32,
}

impl Default for libc::timeval {
    fn default() -> Self {
        Self { tv_sec: 0, tv_usec: 0 }
    }
}

// IOCTL wrappers
const VIDIOC_QUERYCAP: libc::c_ulong = 0x80685600;
const VIDIOC_G_FMT: libc::c_ulong = 0xC0D05604;
const VIDIOC_S_FMT: libc::c_ulong = 0xC0D05605;
const VIDIOC_TRY_FMT: libc::c_ulong = 0xC0D05640;
const VIDIOC_REQBUFS: libc::c_ulong = 0xC0145608;
const VIDIOC_QUERYBUF: libc::c_ulong = 0xC0585609;
const VIDIOC_QBUF: libc::c_ulong = 0xC058560F;
const VIDIOC_DQBUF: libc::c_ulong = 0xC0585611;
const VIDIOC_STREAMON: libc::c_ulong = 0x40045612;
const VIDIOC_STREAMOFF: libc::c_ulong = 0x40045613;

unsafe fn ioctl_querycap(fd: RawFd, cap: *mut v4l2_capability) -> i32 {
    libc::ioctl(fd, VIDIOC_QUERYCAP, cap)
}

unsafe fn ioctl_g_fmt(fd: RawFd, fmt: *mut v4l2_format) -> i32 {
    libc::ioctl(fd, VIDIOC_G_FMT, fmt)
}

unsafe fn ioctl_s_fmt(fd: RawFd, fmt: *mut v4l2_format) -> i32 {
    libc::ioctl(fd, VIDIOC_S_FMT, fmt)
}

unsafe fn ioctl_try_fmt(fd: RawFd, fmt: *mut v4l2_format) -> i32 {
    libc::ioctl(fd, VIDIOC_TRY_FMT, fmt)
}

unsafe fn ioctl_reqbufs(fd: RawFd, req: *mut v4l2_requestbuffers) -> i32 {
    libc::ioctl(fd, VIDIOC_REQBUFS, req)
}

unsafe fn ioctl_querybuf(fd: RawFd, buf: *mut v4l2_buffer) -> i32 {
    libc::ioctl(fd, VIDIOC_QUERYBUF, buf)
}

unsafe fn ioctl_qbuf(fd: RawFd, buf: *mut v4l2_buffer) -> i32 {
    libc::ioctl(fd, VIDIOC_QBUF, buf)
}

unsafe fn ioctl_dqbuf(fd: RawFd, buf: *mut v4l2_buffer) -> i32 {
    libc::ioctl(fd, VIDIOC_DQBUF, buf)
}

unsafe fn ioctl_streamon(fd: RawFd, type_: *const u32) -> i32 {
    libc::ioctl(fd, VIDIOC_STREAMON, type_)
}

unsafe fn ioctl_streamoff(fd: RawFd, type_: *const u32) -> i32 {
    libc::ioctl(fd, VIDIOC_STREAMOFF, type_)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_codes() {
        // Verify format codes are correct
        assert_eq!(format_codes::V4L2_PIX_FMT_YUYV, 0x56595559);
    }

    #[test]
    fn test_query_camera_info_nonexistent() {
        let result = query_camera_info(Path::new("/dev/nonexistent"));
        assert!(result.is_err());
    }
}
