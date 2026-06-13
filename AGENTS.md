# AGENTS.md — SLFAM

Secure Lightweight Facial Authentication Module. Linux PAM module + enrollment CLI, written in Rust. Local, offline, no cloud. Encrypts face templates with XChaCha20-Poly1305.

## Repository Layout

```
faceMe/
├── slfam/                  # Rust crate (the entire implementation)
│   ├── Cargo.toml          # crate-type: ["lib", "cdylib"] — produces .so + .rlib
│   ├── config.example.toml
│   └── src/
│       ├── lib.rs          # Module declarations, re-exports, constants
│       ├── config.rs       # TOML config — Config struct + 8 sub-structs
│       ├── error.rs        # AuthError hierarchy — should_fallback() / is_security_concern()
│       ├── camera/         # V4L2 abstraction (Camera trait, V4l2Camera, MockCamera, Frame)
│       ├── detection/      # RetinaFace + 68-pt landmarks + alignment → AlignedFace 112×112
│       ├── liveness/       # Blink (EAR), optical flow (SAD), LBP texture, IR reflectance
│       ├── embedding/      # MobileFaceNet ONNX → FaceEmbedding (512D f32, zeroized)
│       ├── matching/       # Cosine similarity, SecurityLevel::Normal/High, RateLimiter
│       ├── crypto/         # XChaCha20-Poly1305 AEAD, Argon2id key derivation, DerivedKey
│       ├── template/       # TemplateStore: load/save encrypted .slfm files per user
│       ├── pam/            # PAM C ABI (pam_sm_authenticate), PamHandler, conversation
│       └── bin/
│           └── slfam-enroll.rs  # CLI binary for enrolling users
└── .agents/summary/        # Full documentation set (see index.md)
```

All work happens inside `slfam/`. Run `cargo` commands from `slfam/`, not the repo root.

## Subsystems and Key Entry Points

| Subsystem | Primary File | Entry Point |
|---|---|---|
| PAM authentication | `src/pam/mod.rs` | `pam_sm_authenticate` (C ABI) |
| Auth orchestration | `src/pam/handler.rs` | `PamHandler::authenticate` |
| Enrollment | `src/bin/slfam-enroll.rs` | `fn main()` |
| Full detection pipeline | `src/detection/mod.rs` | `FaceDetectionPipeline::process_frame` |
| Liveness orchestration | `src/liveness/mod.rs` | `LivenessAnalyzer::analyze` |
| Embedding generation | `src/embedding/mobilefacenet.rs` | `EmbeddingGenerator::generate` |
| Template persistence | `src/template/storage.rs` | `TemplateStore::save` / `load` |
| Encryption | `src/crypto/xchacha.rs` | `encrypt` / `decrypt` |
| Config loading | `src/config.rs` | `Config::load_or_default` |

## Repo-Specific Patterns

**Camera is a trait, not a struct.** `Camera` in `camera/mod.rs` is a trait. Use `V4l2Camera` in production, `MockCamera` in tests. Don't call V4L2 ioctls directly outside `camera/v4l2.rs`.

**Error conversion is automatic.** All sub-errors implement `From<SubError> for AuthError`. Functions return `Result<T>` (aliased to `Result<T, AuthError>`). The PAM layer reads `AuthError::should_fallback()` to decide whether to return `PAM_AUTHINFO_UNAVAIL` (triggers password fallback) vs `PAM_AUTH_ERR`.

**DerivedKey and FaceEmbedding are zeroized on drop.** Don't clone them into long-lived structures. Pass by reference where possible.

**PamHandler uses lazy init.** `TemplateStore` and key derivation are not loaded at construction; they are loaded on the first call to `authenticate()`. This avoids PAM load-time overhead.

**Template AAD is the username.** When calling `encrypt`/`decrypt` on template data, the Additional Authenticated Data must be `user_id.as_bytes()`. Mismatched AAD causes decryption failure — this is intentional (prevents template swapping).

**Template files use the `.slfm` extension** and start with magic bytes `b"SLFM"` followed by version byte `0x01`. The payload is `EncryptedData::to_bytes()` wrapping JSON-serialized `Template`.

**Config has a `[dev]` section** with `dev_mode`, `use_mock_camera`, `skip_liveness`, `skip_encryption`. These are all insecure; guard any code that reads them with assertions or feature flags.

## Build Outputs

| Output | Purpose |
|---|---|
| `target/debug/libslfam.so` | PAM module — install to `/lib/security/pam_slfam.so` |
| `target/debug/slfam-enroll` | Enrollment CLI |
| `target/debug/libslfam.rlib` | Rust library for direct linkage |

Release build: `cargo build --release` from `slfam/`. Release profile uses `lto=true`, `strip=true`, `panic="abort"`.

## Cargo Features

| Feature | Effect |
|---|---|
| `v4l2` | Links the `v4l` crate as a higher-level V4L2 backend (off by default; raw ioctls used otherwise) |
| `tpm` | Enables TPM code paths in `crypto/keys.rs` |
| `dev-mode` | Enables development overrides |

## Runtime File Locations

| Path | Role |
|---|---|
| `/etc/slfam/config.toml` | Main config (TOML) |
| `/var/lib/slfam/templates/{user}.slfm` | Encrypted template per user |
| `/usr/share/slfam/models/` | ONNX model files |
| `/var/log/slfam/audit.log` | Auth event log (no biometric data) |

## Known Issues (see `.agents/summary/review_notes.md` for detail)

- `slfam-test-auth` is referenced in README but is **not implemented**. There is only `slfam-enroll`.
- README says `cd slfam/slfam` — the correct path is `cd slfam`.
- `config.example.toml` has `ir_device_id = -1` which **fails to parse** (type is `Option<CameraDevice>`). Omit the field or comment it out to disable IR.
- `[embedding]` and `[pam]` sections in `config.example.toml` are silently ignored (no corresponding config structs exist yet).

## Documentation

Full documentation in `.agents/summary/`. Start with `.agents/summary/index.md`.

## Custom Instructions
<!-- This section is for human and agent-maintained operational knowledge.
     Add repo-specific conventions, gotchas, and workflow rules here.
     This section is preserved exactly as-is when re-running codebase-summary. -->
