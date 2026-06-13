# Technical Stack Implementation

## Core Language
- **Rust** (primary) - Memory-safe, excellent for PAM integration via C ABI, secure by default
- **Python** (prototyping/research) - For CV experimentation and model evaluation

## Computer Vision & ML Inference
- **OpenCV** (C++ with Rust bindings) - Mature, lightweight for image ops, optical flow, LBP
- **ONNX Runtime** - Portable, optimized inference engine for face embedding models
- **dlib** or lightweight CNN - For 68-point facial landmarks

## Face Recognition Models
- **MobileFaceNet** (primary) - Lightweight, optimized for edge devices
- **ArcFace mobile variant** (alternative) - Better accuracy if resources allow

## Cryptography & Key Management
- **libsodium** (via sodiumoxide for Rust) - Modern, easy-to-use XChaCha20-Poly1305 AEAD
- **tpm2-tss** + **tpm2-tools** - TPM hardware binding for device-bound keys
- **zeroize** crate - Memory zeroization for sensitive data

## PAM Integration
- **pam-sys** (Rust) - Write PAM module as `cdylib` exposing C ABI
- Native C shared library interface

## CLI & Tooling
- **clap** (Rust) - For enrollment CLI with minimal binary size
- **serde** + **serde_json** - Configuration and metadata serialization

## Platform-Specific
- **V4L2** (Linux) - Camera device enumeration and capture
- **systemd** - Service management for enrollment daemon (optional)

## Optional Performance Accelerators
- **OpenVINO** (Intel CPUs/GPUs) - If targeting Intel hardware
- **TensorRT** (NVIDIA) - If GPU acceleration needed

## Build & Packaging
- **Cargo** - Rust build system
- **dpkg-deb** - Debian package creation
- **Cross** - Cross-compilation for different architectures

## Testing & Quality
- **cargo test** - Unit testing
- **criterion** - Benchmarking for latency metrics
- **cargo-fuzz** - Fuzzing PAM input vectors
- **valgrind** / **heaptrack** - Memory profiling

## Stack Rationale

1. **Rust core** ensures memory safety, critical for PAM modules that run with elevated privileges
2. **ONNX Runtime** provides model portability and optimization without vendor lock-in
3. **libsodium** offers battle-tested crypto primitives with simple API
4. **TPM integration** provides hardware-backed security for template encryption
5. **Small footprint** - Rust produces compact binaries, ONNX is lightweight, OpenCV can be statically linked
6. **Performance** - Meets <2s latency target with <30% CPU utilization
7. **Security-first** - Memory-safe language + modern crypto + hardware binding

## Acceptance Criteria Alignment

- **<25MB binary** ✓ Rust + static linking
- **<8KB templates** ✓ 128-512D float embeddings encrypted
- **<2s latency** ✓ ONNX optimized inference
- **Offline-only** ✓ No cloud dependencies
- **Strong security** ✓ Memory-safe + TPM + AEAD encryption
