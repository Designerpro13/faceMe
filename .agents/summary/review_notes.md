# Review Notes — SLFAM Documentation

## Consistency Checks

### Consistent

- **Module structure matches code**: All 10 modules documented in `components.md` correspond to actual `pub mod` declarations in `lib.rs`.
- **Trait implementations**: `Camera`, `KeyDerivation`, and `PamConversation` traits documented in `interfaces.md` match the structs implementing them as found in source.
- **Error hierarchy**: `data_models.md` error tree matches the `#[from]` conversions in `error.rs`.
- **Config sections**: All 8 `Config` sub-structs documented in `data_models.md` match the TOML config and Rust structs.
- **Template file format**: Magic bytes (`b"SLFM"`), version (`1`), and AAD strategy documented in `data_models.md` match constants in `lib.rs` and logic in `template/storage.rs`.
- **EMBEDDING_DIM = 512**: Consistent across `lib.rs`, `mobilefacenet.rs`, and documentation.
- **Cargo features**: `v4l2`, `tpm`, `dev-mode` features documented in `codebase_info.md` match `Cargo.toml`.

### Minor Inconsistencies

**1. `ir_device_id` type mismatch in config**
- `config.example.toml` uses `ir_device_id = -1` (signed integer) to indicate "not available".
- `config.rs` defines `ir_device_id: Option<CameraDevice>` which uses `None` for absence.
- The TOML parser will fail to parse `-1` as `Option<CameraDevice>`. The example config appears to be aspirational rather than tested.

**2. `[embedding]` section in config.example.toml vs config.rs**
- `config.example.toml` has a `[embedding]` section with `embedding_dim`, `model_file`, `input_size`.
- `config.rs` does not define an `EmbeddingConfig` struct; embedding config lives under `[detection]` or defaults are hardcoded in the `EmbeddingGenerator`.
- **Impact**: Users following the example config will have an unrecognized `[embedding]` section ignored by `toml`'s `#[serde(default)]` behavior (no error, but silent no-op).

**3. `[pam]` section in config.example.toml vs config.rs**
- `config.example.toml` has `[pam]` section with `auth_timeout_sec`, `show_feedback`, `enable_fallback`, `prompt`.
- `config.rs` does not define a `PamConfig` struct; timeout is read from `PamArgs` (PAM module arguments) and camera config.
- Same silent no-op issue.

**4. Phase numbering gap in PROGRESS.md**
- Phases listed: 0, 1, 2, 3, 5, 6. Phase 4 (IR Integration) is listed as optional/future but the numbering is non-sequential. Not a code issue but may cause confusion.

---

## Completeness Checks

### Well Documented

- Authentication and enrollment flows (full sequence diagrams in `workflows.md`)
- Liveness pipeline with all four signals
- Crypto layer (AEAD scheme, key derivation, zeroize)
- PAM C ABI exports and return code mapping
- Config surface area

### Missing: `slfam-test-auth` Binary

`README.md` documents a `slfam-test-auth` command:
```bash
sudo ./target/release/slfam-test-auth --user $USER
```
This binary does **not exist**in the codebase. Only `slfam-enroll` is defined in `Cargo.toml`. This is either unimplemented or was removed. Users following the README will hit a "file not found" error.

**Recommendation:**Either implement `slfam-test-auth` as a second `[[bin]]` entry in `Cargo.toml`, or update `README.md` to remove this step and describe how to test using `dev_mode = true` + PAM debugging tools.

### Missing: No Workspace `Cargo.toml`

The project root (`faceMe/`) has no `Cargo.toml`. There is only `slfam/Cargo.toml`. This means:
- `cargo build` must be run from `slfam/`, not from the project root.
- The README's `cd slfam/slfam && cargo build` instruction suggests an extra directory level that doesn't exist (the structure is `faceMe/slfam/`, not `faceMe/slfam/slfam/`).

**Recommendation:**Fix the README path to `cd slfam && cargo build --release`, or add a workspace `Cargo.toml` at the root for convenience.

### Partial: Model File Names Not Validated

`config.example.toml` specifies model filenames like `retinaface.onnx`, `landmarks_68.onnx`, `mobilefacenet.onnx`, `lbp_classifier.onnx`. These names are not validated against the actual model loading code in `detection/onnx.rs` and `embedding/mobilefacenet.rs`, which read paths from config. If the actual downloaded models have different names, silent failures will occur at model load time.

**Recommendation:**Document exact expected filenames in `MODEL_SETUP.md` and add validation in `Config::validate()` to check model file existence.

### Partial: `DetectionConfig` fields not fully documented

`config.rs` defines `DetectionConfig` but the exact fields are not visible in the portion read. Fields like `confidence_threshold`, `nms_threshold`, `detection_model`, `landmark_model` are referenced in `config.example.toml` and components but weren't extracted from source. Documentation may be missing some config fields.

**Recommendation:**Read the full `DetectionConfig` struct and ensure all fields are documented in `data_models.md`.

### Partial: Liveness `FrameData` struct undocumented

`LivenessAnalyzer` uses a `FrameData` struct (seen in `liveness/mod.rs` symbols) that bundles a frame with its landmarks. This internal type is not documented — relevant if extending the liveness module.

### Partial: Enrollment variation requirements unspecified

`EnrollmentConfig` has a `require_pose_variation` flag (in config.example.toml) but the algorithm for measuring/enforcing pose variation during enrollment is not documented. The `SimpleLandmarkEstimator` and `rotation_angle` function in `detection/landmarks.rs` likely play a role.

### Partial: No documented rollback/emergency disable mechanism

`Config::is_emergency_disabled()` exists but the mechanism (what file/flag it checks) is not documented. The README mentions an `slfam-disable` script at `/usr/bin/slfam-disable` which also doesn't exist in the codebase.

### ℹ Language Coverage

- **100% Rust**: Single language, single crate. No FFI beyond the PAM C ABI export. No shell scripts, Python, or other languages in the analyzed source. Documentation coverage is complete for all Rust modules.

---

## Recommendations

| Priority | Action |
|---|---|
| High | Add `slfam-test-auth` binary or remove from README |
| High | Fix README `cd` path — should be `cd slfam`, not `cd slfam/slfam` |
| High | Fix `ir_device_id = -1` in config.example.toml (use `# ir_device_id = 1` commented out) |
| Medium | Remove undocumented `[embedding]` and `[pam]` TOML sections or implement the corresponding config structs |
| Medium | Document `Config::is_emergency_disabled()` behavior and provide the `slfam-disable` script |
| Medium | Validate model file existence in `Config::validate()` |
| Low | Add workspace `Cargo.toml` at project root for developer convenience |
| Low | Document `FrameData` struct and enrollment pose variation algorithm |
| Low | Fix PROGRESS.md phase numbering (rename Phase 4 → something that doesn't imply phases 4 is skipped) |
