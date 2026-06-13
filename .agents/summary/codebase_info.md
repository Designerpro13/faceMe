# Codebase Info — SLFAM

## Project Identity

| Field | Value |
|---|---|
| Name | slfam |
| Version | 0.1.0 |
| License | MIT |
| Language | Rust (edition 2021) |
| Minimum Rust | 1.70+ |
| Platform | Linux (x86_64, ARM64) |
| Outputs | `libslfam.so` (PAM module), `libslfam.rlib` (library), `slfam-enroll` (CLI binary) |

## Repository Layout

```
faceMe/
├── slfam/                    # Rust crate (main workspace)
│   ├── Cargo.toml
│   ├── config.example.toml
│   └── src/
│       ├── lib.rs            # Library root, re-exports, constants
│       ├── config.rs         # TOML config management
│       ├── error.rs          # Error hierarchy (thiserror)
│       ├── camera/           # V4L2 camera abstraction
│       │   ├── mod.rs        # Camera trait, DeviceLock
│       │   ├── device.rs     # CameraDevice, CameraInfo, CameraType
│       │   ├── frame.rs      # Frame, FrameFormat
│       │   ├── v4l2.rs       # V4l2Camera (ioctl-based)
│       │   └── mock.rs       # MockCamera (testing)
│       ├── detection/        # Face detection & alignment
│       │   ├── mod.rs        # FaceDetectionPipeline, BoundingBox
│       │   ├── retinaface.rs # RetinaFace detector
│       │   ├── landmarks.rs  # 68/5-point landmark detection
│       │   ├── alignment.rs  # Face alignment (similarity transform)
│       │   └── onnx.rs       # ONNX model runner + preprocessing
│       ├── liveness/         # Anti-spoofing checks
│       │   ├── mod.rs        # LivenessAnalyzer, orchestration
│       │   ├── blink.rs      # EAR-based blink detection
│       │   ├── optical_flow.rs # SAD-based optical flow
│       │   ├── lbp.rs        # LBP texture analysis
│       │   └── ir.rs         # IR reflectance analysis
│       ├── embedding/        # Face embedding generation
│       │   ├── mod.rs        # Math utilities (cosine, euclidean, L2)
│       │   └── mobilefacenet.rs # MobileFaceNet ONNX inference
│       ├── matching/         # Embedding comparison
│       │   └── mod.rs        # Matcher, SecurityLevel, MatchResult
│       ├── crypto/           # Encryption & key management
│       │   ├── mod.rs        # Utilities (nonce, random bytes)
│       │   ├── xchacha.rs    # XChaCha20-Poly1305 AEAD, EncryptedData
│       │   └── keys.rs       # DerivedKey, PasswordKeyDerivation, TpmKeyDerivation
│       ├── template/         # Encrypted template storage
│       │   ├── mod.rs        # create_template / load_template helpers
│       │   └── storage.rs    # TemplateStore, Template, TemplateMetadata
│       ├── pam/              # PAM module entry points
│       │   ├── mod.rs        # pam_sm_authenticate, PamResultCode
│       │   ├── handler.rs    # PamHandler, RateLimiter
│       │   └── conversation.rs # PAM conversation protocol
│       └── bin/
│           └── slfam-enroll.rs # Enrollment CLI (clap)
├── README.md
├── MODEL_SETUP.md
├── SoftReq&Goals.md
├── TechStack.md
├── ProcessChecklist.md
├── PROGRESS.md
├── Instructions.md
└── LLM_Limitations.md
```

## Runtime File Locations

| Path | Purpose |
|---|---|
| `/etc/slfam/config.toml` | Main configuration (TOML) |
| `/var/lib/slfam/templates/` | Encrypted face templates per user |
| `/usr/share/slfam/models/` | ONNX model files |
| `/var/log/slfam/audit.log` | Audit log (no biometric data) |
| `/lib/security/pam_slfam.so` | Installed PAM module |

## Cargo Features

| Feature | Effect |
|---|---|
| `v4l2` | Enable real V4L2 camera support (links `v4l` crate) |
| `tpm` | Enable TPM 2.0 key binding |
| `dev-mode` | Enable development/testing overrides |

## Constants (lib.rs)

| Constant | Value | Purpose |
|---|---|---|
| `VERSION` | `CARGO_PKG_VERSION` | Library version |
| `TEMPLATE_MAGIC` | `b"SLFM"` | Template file magic bytes |
| `TEMPLATE_VERSION` | `1` | Template format version |
| `EMBEDDING_DIM` | `512` | MobileFaceNet output dimension |

## Build Outputs

- **`libslfam.so`** — cdylib PAM module, installed to `/lib/security/`
- **`libslfam.rlib`** — Rust library for direct integration
- **`slfam-enroll`** — CLI binary for enrolling users

## Test Infrastructure

- Unit tests inline in each module
- Mock camera (`MockCamera`) for hardware-free testing
- Mock embedding generator (`MockEmbeddingGenerator`)
- `dev-mode` feature flag for bypassing hardware checks
- Optional features: `mock_camera`, `real_camera` for integration tests
