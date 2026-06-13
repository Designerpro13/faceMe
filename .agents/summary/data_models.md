# Data Models — SLFAM

## Core Structs

### `Config` (`slfam/src/config.rs`)

Top-level configuration. Loaded from TOML, deserialized with `serde`.

```
Config
├── general: GeneralConfig
│   ├── template_dir: PathBuf        (/var/lib/slfam/templates)
│   ├── model_dir: PathBuf           (/usr/share/slfam/models)
│   ├── log_file: PathBuf            (/var/log/slfam/audit.log)
│   ├── log_level: String            ("info")
│   └── debug_mode: bool             (false)
├── camera: CameraConfig
│   ├── device_id: CameraDevice      (Index(0) | Path(String))
│   ├── ir_device_id: Option<CameraDevice>
│   ├── device_ids: Vec<u32>
│   ├── capture_timeout_ms: u64
│   ├── timeout_secs: u64
│   ├── frame_width: u32
│   ├── frame_height: u32
│   ├── fps: u32
│   ├── prefer_ir: bool
│   └── auto_detect: bool
├── detection: DetectionConfig
│   ├── confidence_threshold: f32
│   ├── nms_threshold: f32
│   └── model paths (face_detector, landmark_detector)
├── liveness: LivenessConfig
│   ├── require_blink: bool
│   ├── enable_lbp: bool
│   ├── enable_optical_flow: bool
│   ├── enable_ir: bool
│   └── per-signal thresholds
├── matching: MatchingConfig
│   ├── threshold_normal: f32        (0.75)
│   ├── threshold_high_security: f32 (0.85)
│   └── min_samples: usize
├── security: SecurityConfig
│   ├── max_attempts: u32
│   ├── lockout_duration_sec: u64
│   └── use_tpm: bool
└── enrollment: EnrollmentConfig
    └── num_samples: usize
```

---

### `Frame` (`slfam/src/camera/frame.rs`)

Raw camera frame. Owns its pixel buffer.

| Field | Type | Notes |
|---|---|---|
| `data` | `Vec<u8>` | Pixel buffer |
| `width` | `u32` | |
| `height` | `u32` | |
| `format` | `FrameFormat` | `BGR24` / `YUYV` / `Grayscale` |
| `timestamp` | `Instant` | Capture time |
| `sequence` | `u32` | V4L2 buffer sequence number |

```rust
pub enum FrameFormat {
    BGR24,
    YUYV,
    Grayscale,
}
```

---

### `ProcessedFace` (`slfam/src/detection/mod.rs`)

Result of the full detection pipeline for one frame.

| Field | Type | Notes |
|---|---|---|
| `bounding_box` | `BoundingBox` | x, y, w, h |
| `confidence` | `f32` | Detector confidence (0–1) |
| `landmarks` | `FaceLandmarks` | 68-point or 5-point |
| `aligned` | `AlignedFace` | 112×112 BGR crop |

### `BoundingBox`

```rust
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
```

Methods: `center()`, `area()`, `contains(point)`, `expand(factor)`, `iou(other)`, `to_rect()`.

### `FaceLandmarks` (`slfam/src/detection/landmarks.rs`)

Structured access to face keypoints. Supports both 68-point (full model) and 5-point (lightweight).

| Accessor | Points | Purpose |
|---|---|---|
| `left_eye()` | 6 pts (68-pt) | Blink EAR |
| `right_eye()` | 6 pts (68-pt) | Blink EAR |
| `left_eye_center()` | 1 pt | Alignment |
| `right_eye_center()` | 1 pt | Alignment |
| `nose_tip()` | 1 pt | Alignment |
| `mouth()` | pts | Alignment |
| `jaw()` | 17 pts | Pose estimation |
| `five_point()` | 5 pts | Fast alignment |

---

### `AlignedFace` (`slfam/src/detection/alignment.rs`)

Cropped, affine-aligned face image.

| Field | Type | Notes |
|---|---|---|
| `data` | `Vec<u8>` | BGR24 pixels |
| `width` | `u32` | 112 |
| `height` | `u32` | 112 |

---

### `FaceEmbedding` (`slfam/src/embedding/mobilefacenet.rs`)

512-dimensional face descriptor. Zeroized on drop.

| Field | Type | Notes |
|---|---|---|
| `data` | `Vec<f32>` | L2-normalized, length 512 |

```rust
// Serialization: raw little-endian f32 bytes
// 512 × 4 bytes = 2048 bytes per embedding
embedding.to_bytes()     // Vec<u8>, 2048 bytes
FaceEmbedding::from_bytes(&bytes)  // must be exactly 2048 bytes
```

---

### `MatchResult` (`slfam/src/matching/mod.rs`)

| Field | Type | Notes |
|---|---|---|
| `matched` | `bool` | Whether similarity ≥ threshold |
| `similarity` | `f32` | Cosine similarity (0–1) |
| `threshold` | `f32` | Threshold used |
| `duration` | `Duration` | Match computation time |
| `details` | `MatchDetails` | Best index, all scores, security level |

---

### `Template` and `TemplateMetadata` (`slfam/src/template/storage.rs`)

```
Template
├── user_id: String
├── embeddings: Vec<FaceEmbedding>   (typically 5 samples)
├── metadata: TemplateMetadata
│   ├── created_at: u64              (Unix epoch seconds)
│   ├── updated_at: u64
│   ├── auth_count: u64
│   ├── last_auth: Option<u64>
│   ├── device_id: Option<String>
│   └── extra: HashMap<String, String>
└── version: u8                      (= TEMPLATE_VERSION = 1)
```

---

### `EncryptedData` (`slfam/src/crypto/xchacha.rs`)

Wire format for all encrypted blobs.

| Field | Type | Notes |
|---|---|---|
| `nonce` | `[u8; 24]` | 192-bit random nonce (XChaCha20) |
| `ciphertext` | `Vec<u8>` | Encrypted + authenticated bytes |

```
Binary layout (to_bytes / from_bytes):
[4 bytes: nonce_len][24 bytes: nonce][4 bytes: ciphertext_len][N bytes: ciphertext]
```

---

### `DerivedKey` (`slfam/src/crypto/keys.rs`)

```rust
// 32-byte (256-bit) key
// ZeroizeOnDrop: memory wiped when dropped
// Debug impl prints "[REDACTED]"
struct DerivedKey { key: [u8; 32] }
```

---

## Template File Format

Template files are stored at `{template_dir}/{user_id}.slfm`.

```
File binary layout:
[4 bytes]  Magic: b"SLFM"
[1 byte]   Version: 0x01
[N bytes]  EncryptedData (serialized)
           └── Plaintext (before encryption):
               JSON-serialized Template struct
               (embeddings as base64, metadata as JSON)
```

The AAD (Additional Authenticated Data) for AEAD is the `user_id` string bytes, binding the ciphertext to the specific user and preventing template swapping attacks.

---

## Liveness Data Models

### `LivenessResult`

| Field | Type |
|---|---|
| `passed` | `bool` |
| `blink` | `Option<CheckResult>` |
| `optical_flow` | `Option<CheckResult>` |
| `lbp_texture` | `Option<CheckResult>` |
| `ir_reflectance` | `Option<CheckResult>` |

### `CheckResult`

| Field | Type |
|---|---|
| `passed` | `bool` |
| `score` | `f32` |
| `reason` | `Option<String>` |

---

## Error Types (`slfam/src/error.rs`)

```mermaid
classDiagram
    class AuthError {
        Camera(CameraError)
        Detection(DetectionError)
        Liveness(LivenessError)
        Embedding(EmbeddingError)
        Crypto(CryptoError)
        Template(TemplateError)
        Matching(MatchingError)
        Config(ConfigError)
        Pam(PamError)
        RateLimited{lockout_seconds, failed_attempts}
        Io(std::io::Error)
        AuthenticationFailed
        Internal(String)
    }
    class CameraError {
        NoDevice
        DeviceNotFound(PathBuf)
        OpenFailed(String)
        CaptureFailed(String)
        FormatNotSupported
        Timeout
        StreamError(String)
        DeviceLocked
    }
    class CryptoError {
        EncryptionFailed
        DecryptionFailed
        InvalidKeyLength{expected, got}
        TpmUnavailable
        KeyNotFound(PathBuf)
    }
    class TemplateError {
        NotFound(String)
        CorruptData
        WrongKey
        StorageFull
        InvalidVersion{expected, got}
    }
    AuthError --> CameraError
    AuthError --> CryptoError
    AuthError --> TemplateError
```
