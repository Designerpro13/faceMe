# Secure Lightweight Facial Authentication Module — Technical Blueprint (fine-grained)

Below is a comprehensive, engineering-grade implementation document for a **stronger, smaller, privacy-preserving face unlock subsystem** with IR support and RGB fallback. It covers technical aspects, phase-based implementation steps, checklists, data-flow models, user journeys, test metrics, and final precautions. I will suggest top-class frameworks and stacks without locking you into a single implementation language.

---

# 1. Executive summary (one line)

Design a local, offline facial authentication PAM subsystem that fuses IR + RGB, enforces multi-signal liveness, stores encrypted templates device-bound to a secure key (TPM if available), and always falls back to UNIX password — optimized for security, small footprint, and practical deployability.

---

# 2. High-level objectives & acceptance criteria

**Objectives**

* Local processing only; no cloud calls.
* Multi-signal liveness (blink + optical flow + texture + IR reflectance).
* Encrypted, tamper-evident templates bound to device.
* PAM integration: `auth sufficient pam_slfam.so` then `pam_unix.so` fallback.
* Usable in <2s auth latency on typical laptop CPU.
* Minimal runtime dependencies; memory-safe core.

**Acceptance metrics**

* False Accept Rate (FAR) ≤ 0.1% (target; tunable).
* False Reject Rate (FRR) ≤ 2–5% (with enrollment of multiple samples).
* End-to-end auth latency ≤ 2s (median).
* Template file size ≤ 8KB per user (128–512D float embedding encrypted).
* Binary size (core) < 25 MB (target).
* Max CPU utilization < 30% during active auth.
* Lockout on >5 failed attempts with cooldown 15 min.

---

# 3. System components (logical)

1. **Camera Abstraction Layer**

   * Detects /dev/video* devices; identifies IR camera vs RGB.
   * Ensures exclusive access and closes FDs promptly.

2. **Capture Engine**

   * Frame sampler, pre-processor (resize, color convert), face cropper.

3. **Face Detector & Landmarker**

   * Detects face and landmarks for alignment and eye region extraction.

4. **Liveness Stack**

   * Blink detector (EAR), optical flow depth heuristic, texture (LBP) classifier, IR reflectivity test, optional challenge prompts.

5. **Embedding Generator**

   * Runs compact face embedding model (MobileFaceNet / ArcFace variants).

6. **Template Manager**

   * Template encryption, versioning, integrity (HMAC), storage.

7. **Matching Engine**

   * L2 normalization + cosine similarity; adaptive thresholds.

8. **PAM Integration Module**

   * Exposes PAM C ABI; orchestrates capture → liveness → match → verdict.

9. **Admin & Enrollment CLI**

   * Enrollment UX, template management, audit logs (no raw biometric data).

10. **Key Management**

    * Device master key via TPM or derived from protected machine identity; fallback to disk protected key using libsodium.

11. **Ops & Monitoring**

    * Embedded telemetry (counts only), fail counters, rate limiter hooks.

---

# 4. Top framework & technology choices (supported, high-quality options)

> Don’t finalize language; choose per team skill and environment. Each below is proven for this domain.

**Computer Vision / ML inference**

* OpenCV (core image ops, optical flow, LBP) — mature C/C++ with many bindings.
* ONNX Runtime — portable inference for ONNX models (small, optimized).
* TensorFlow Lite (optional) — if using TFLite models on edge devices.
* PyTorch Mobile / LibTorch — for teams entrenched in PyTorch.

**Face models (embedding)**

* MobileFaceNet (lightweight)
* ArcFace (resnet or mobile variants)
* FaceNet (classic) — larger but accurate

**Liveness / Landmark**

* MediaPipe (fast landmarking) — optional; heavier binary but robust.
* dlib (68-point landmarks) — C++ proven.
* Lightweight landmark networks (tiny CNNs) served via ONNX.

**Crypto & Key Management**

* libsodium / sodiumoxide (Rust) — modern, easy-to-use primitives.
* OpenSSL (if already in stack) — more complex API.
* TPM: tss2 (tpm2-tss) + tpm2-tools for hardware binding.

**PAM integration**

* Native C shared lib (libpam) — simplest for PAM.
* Rust: `pam-sys` / write C ABI via `cdylib` — memory safe and recommended.
* Go or C++ possible but interop with PAM is easiest in C ABI.

**Language choices (per module)**

* Core (PAM + matching/crypto): Rust or C++ (Rust preferred for safety).
* CV prototyping & research: Python + OpenCV + ONNX.
* Enrollment CLI: Rust/Go/CLI framework (e.g., clap, cobra) for small binaries.

**Optional performance accelerators**

* OpenVINO (Intel) or TensorRT (NVIDIA) for faster inference on those platforms.

---

# 5. Data model & file formats

**Template file structure** (`/var/lib/slfam/templates/<user>.bin`)

```
Header:
  magic: 4 bytes ("SLFM")
  version: u8
  flags: u8 (bitmask: has_ir, has_rgb)
  key_meta: json (kdf parameters, tpm bound id)  — optional

Payload:
  nonce: 24 bytes (XChaCha20-Poly1305)
  ciphertext: variable (embedding serialized)
  mac: 16 bytes (poly1305)
  metadata: json (enroll date, model id, enroll_samples, salt)
```

**Embedding serialization**

* Quantize floats to 16-bit (optional) or store as 32-bit float array.
* L2 normalized.
* Store model_id and version to support rolling updates.

**Audit log (not biometric)**

* `/var/log/slfam/audit.log` entries: `[timestamp] user action result source`
* Never log raw frames, embeddings, or PII.

---

# 6. Phase-based implementation plan (detailed tasks, checklists)

### Phase 0 — Project setup & R&D

* [ ] Prepare repo & CI templates; static analysis enabled.
* [ ] Build minimal OpenCV and ONNX runtime cross-compile tests.
* [ ] Select initial embedding model and verify on sample dataset.
* [ ] Select primary language for core (Rust recommended).

**Deliverables:** minimal inference demo, PoC for detection+embedding.

---

### Phase 1 — Capture & Detector

* [ ] Camera device enumerator & selector (IR vs RGB).
* [ ] Secure capture loop with frame TTL & exclusive locking.
* [ ] Face detector + face crop + alignment.
* [ ] Unit tests for multi-face and no-face conditions.

**Checklist**

* [ ] Camera lock works; FD closed after call.
* [ ] Proper device detection on Linux (V4L2).
* [ ] Face detection recall ≥ 98% on lab set.

---

### Phase 2 — Embedding & Matching

* [ ] Integrate ONNX runtime and model loader.
* [ ] Implement alignment & preprocessing (112×112 or model input).
* [ ] Implement L2 normalization & cosine similarity function.
* [ ] Threshold tuning utility (batch evaluation script).

**Checklist**

* [ ] EER report generated for target dataset.
* [ ] Matching latency measured and within budget.

---

### Phase 3 — Liveness Stack (RGB)

* [ ] Blink detection (EAR) module with per-frame EAR computation.
* [ ] Optical flow variance test (Lucas-Kanade) across face regions.
* [ ] LBP texture classifier trained on printed vs real skin samples.
* [ ] Challenge-response optional module (random micro head turn).

**Checklist**

* [ ] Liveness false acceptance (photo/video) rate validated.
* [ ] Liveness returns decision within 500–800ms.

---

### Phase 4 — IR integration (if IR hardware available)

* [ ] IR camera frame capture + synchronised sampling with RGB (if both present).
* [ ] Compute IR reflectance signature (average ROI intensity + variance curve).
* [ ] IR vs display classifier (detect screens/LCD/OLED).
* [ ] IR+RGB fusion rules: if IR available, weight IR signal higher.

**Checklist**

* [ ] IR detector reduces spoofing false accepts by X% (measure).
* [ ] Graceful fallback: IR missing → stricter RGB thresholds.

---

### Phase 5 — Template storage & crypto

* [ ] Derive device master key (TPM binding preferred).
* [ ] Implement XChaCha20-Poly1305 (libsodium) encrypt/decrypt routines.
* [ ] HMAC or AEAD integrity verification.
* [ ] Implement per-template versioning and rollback protection.

**Checklist**

* [ ] Templates decrypt only on same device with correct key.
* [ ] Tests for tamper detection; corrupt files are rejected.

---

### Phase 6 — PAM module & CLI

* [ ] Implement PAM C ABI wrapper (pam_sm_authenticate, pam_sm_setcred).
* [ ] Implement enrollment CLI with guided multi-angle capture.
* [ ] Add configuration options (thresholds, timeout, lockout).
* [ ] Implement fail counters and integration with `pam_faillock.so` if available.

**Checklist**

* [ ] PAM return codes follow libpam contract.
* [ ] Failover to `pam_unix.so` confirmed.

---

### Phase 7 — Hardening & Testing

* [ ] Memory zeroization for embeddings and secrets (use zeroize).
* [ ] CPU & memory profiling; reduce dynamic dependencies.
* [ ] Penetration tests: photo, video, mask, printing, screen attacks.
* [ ] Fuzzing for PAM input vectors.

**Checklist**

* [ ] No sensitive data in core dumps or logs.
* [ ] Rate limiter & lockout functional.

---

### Phase 8 — Ops, Packaging & Deployment

* [ ] Build packages (.deb) with minimal dependencies.
* [ ] Systemd service or socket (if needed for enrollment).
* [ ] Packaging includes PAM config snippets and installation script.
* [ ] Documentation: admin, enrollment UX, emergency disable switch.

**Checklist**

* [ ] Upgrade path for model & template schema.
* [ ] Uninstall removes templates and config safely.

---

# 7. Data-flow models (diagrams + commentary)

### Component sequence (authentication)

```
[Login(GDM)] → PAM → pam_slfam.so
   → Camera Abstraction → Capture Engine → Face Detector
   → Liveness Stack (RGB/IR fusion)
   → Embedding Generator (model)
   → Template Manager (decrypt stored template)
   → Matching Engine (cosine similarity)
   → Verdict → PAM returns PAM_SUCCESS or PAM_AUTH_ERR
   → On failure within allowed tries → pam_unix.so fallback (password).
```

### ASCII data flow with parallel IR branch

```
        +-----------+
        |  Camera   |
        |(IR+RGB)   |
        +----+------+
             |
      +------v--------+          +-----------------+
      | Capture Engine|---RGB--->| Face Detector   |
      +------+--------+          +--------+--------+
             |                             |
             |----IR----> IR Reflectance   |
             |            Analyzer         |
             |                             v
             |                      +------+-------+
             +--------------------->| Liveness     |
                                    | Fusion Module|---+
                                    +------+-------+   |
                                           |           |
                                           v           |
                                   +-------+-------+   |
                                   | Embedding Gen |<--+
                                   +-------+-------+
                                           |
                                   +-------v-------+
                                   | Template Store|
                                   | (decrypt)     |
                                   +-------+-------+
                                           |
                                   +-------v-------+
                                   | Matcher       |
                                   +-------+-------+
                                           |
                                   +-------v-------+
                                   | Verdict (PAM) |
                                   +---------------+
```

---

# 8. User journey diagram (states & transitions)

**States**

1. Idle / lock screen
2. Face detected (candidate)
3. Liveness check
4. Match evaluation
5. Auth success → session unlocked
6. Auth fail → retry or fallback
7. Fallback password prompt
8. Locked out (policy)

**Transitions**

* Idle → Face detected: camera sees face, single-face requirement
* Face detected → Liveness: run blink + motion + IR
* Liveness → Match: if live, generate embedding and match
* Match → Success: if similarity ≥ threshold
* Match → Fail: if similarity < threshold → retry up to N
* Fail → Fallback: after timeout or configured immediate fallback → password prompt
* Fail → Locked out: after configured failed attempts → cooldown

**User enrollment journey**

* Start enrollment via CLI or settings
* Confirm device authenticity (admin)
* Capture N samples across poses & lighting
* Template generated → encrypt & store
* Enrollment complete confirmation

---

# 9. Detailed algorithms & heuristics (practical formulas)

**EAR (Eye Aspect Ratio)**

* Landmarks: p1–p6 around eye
* `EAR = (||p2−p6|| + ||p3−p5||) / (2 * ||p1−p4||)`
* Blink detected if `EAR < 0.2` for 2–4 consecutive frames (tunable).

**Optical flow variance**

* Compute flow vectors for 3 ROIs (left cheek, nose, right cheek).
* Compute per-ROI average vector and variance across ROIs.
* If variance < threshold → suspect flat surface (reject).

**LBP texture classifier**

* Use 8,1 LBP; compute histogram; normalize; train SVM/LightGBM on labelled real vs printed.

**IR reflectance test**

* For IR frame ROI, compute normalized reflectance profile over 5 frames.
* For human skin, expect certain mean and variance range; screens and photos differ significantly.

**Cosine similarity**

* `score = dot(u, v) / (||u|| * ||v||)` (with unit L2 normalized vectors).
* Operational thresholds: tune on representative in-house dataset. E.g., 0.75 normal, 0.85 high security.

---

# 10. Sample template encryption pseudocode

```text
// Derive device key (TPM preferred)
if TPM available:
  device_key = tpm_unwrap_key(key_handle)
else:
  device_key = KDF(machine_id || salt, iterations)

// Serialize embedding -> bytes
plaintext = serialize(embedding_f32_array)

// AEAD encryption
nonce = random_bytes(24)
ciphertext = XChaCha20Poly1305_Seal(plaintext, aad=header_json, nonce, key)

// Store: header_json | nonce | ciphertext
```

Integrity ensured by AEAD tag; header contains model id/version.

---

# 11. Testing plan (unit, integration, metrics)

**Unit tests**

* Detector false positives/negatives
* EAR thresholds across sample frames
* Encryption/decryption correctness & tamper rejection
* PAM return codes

**Integration tests**

* Simulated login flows in VM with GDM
* Enrollment → delete template → ensure fail
* IR+RGB fusion path tests (with and without IR)

**Adversarial tests**

* Photo printed test (high-quality print)
* Video replay on phone/tablet
* Mask tests (latex / 3D printed)
* Screen replay tests (LCD/OLED)

**Metrics to gather**

* FAR, FRR, EER (ROC curve)
* Latency distribution (p50, p90, p99)
* CPU and memory usage during auth
* Number of false liveness accepts

---

# 12. Deployment & ops considerations

* Provide **emergency disable mechanism**: a local file `/etc/suspend_slfam` or `systemctl disable pam_slfam` script for admins to quickly remove from PAM if misconfigured. Ensure admin instruction to re-enable carefully.
* Rolling updates require template migration path: keep backward compatible format versioning.
* Package as `.deb` with postinst PAM config insertion but require manual admin confirmation (do not auto-switch PAM for safety).
* Provide telemetry opt-in for crash counts only; never transmit biometric artifacts.

---

# 13. Audit & compliance notes

* Comply with local privacy laws: store consent logs, allow deletion of biometric template on request.
* Provide clear admin documentation on data retention and template removal.
* Recommend periodic security audits / third-party code review before production.

---

# 14. Necessary precautions & hard limitations (final)

**Precautions**

* **Backup password**: Always keep a known-good password and account recovery, because a PAM misconfiguration can lock out all users. Test in a VM before production.
* **TPM use is strongly recommended**: Binding templates to TPM significantly raises attacker cost.
* **Do not log or persist raw images or embeddings.** Logs must contain only events and non-sensitive metadata.
* **Rate limit attempts and integrate with `pam_faillock` or equivalent** to prevent brute forcing.
* **Graceful fallback:** make sure the system always falls back to password; ensure `pam_unix.so` remains in stack.
* **Emergency disable:** shipping an “escape hatch” to disable the biometrics quickly is mandatory for production.
* **Model updates:** include versioning and migration; changing model requires re-enrollment or careful migration.
* **User education:** inform users about biometric limits (twins, masks, low light, glasses), and allow easy disablement.
* **Threat boundary:** local attacker with root compromises templates and keys — out of scope; assume physical or root compromise is highest threat and treat accordingly (e.g., full disk encryption).

**Hard limitations**

* Commodity RGB webcams cannot match Apple FaceID or Windows Hello IR + secure enclave. IR cameras and TPM mitigate many attacks but do not reach hardware depth-projector SOTA.
* Biometric auth is **convenience + security** — not absolute identity proof. Combine with hardware token (FIDO2) for high assurance.

---

# 15. Next deliverables I can produce (pick any)

* PAM module skeleton in Rust (complete `pam_sm_authenticate` with error handling).
* Full ONNX inference pipeline example (cross-platform).
* Liveness module code (EAR, optical flow, LBP classifier) with unit tests.
* Enrollment CLI UX mockups and config file schema.
* TPM key binding design and tss2 sample integration.
* Example `.deb` packaging and PAM installation script.

Tell me which of the above you want first and I’ll produce that artifact immediately (Rust PAM skeleton recommended as the next step).
