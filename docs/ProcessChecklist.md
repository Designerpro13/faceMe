# Fine-Grained Process Checklist

## Phase 0 — Project Setup & R&D

- [ ] Prepare repo & CI templates; static analysis enabled
- [ ] Build minimal OpenCV and ONNX runtime cross-compile tests
- [ ] Select initial embedding model and verify on sample dataset
- [ ] Select primary language for core (Rust recommended)

**Deliverables:** minimal inference demo, PoC for detection+embedding

---

## Phase 1 — Capture & Detector

- [ ] Camera device enumerator & selector (IR vs RGB)
- [ ] Secure capture loop with frame TTL & exclusive locking
- [ ] Face detector + face crop + alignment
- [ ] Unit tests for multi-face and no-face conditions

**Validation Checklist:**
- [ ] Camera lock works; FD closed after call
- [ ] Proper device detection on Linux (V4L2)
- [ ] Face detection recall ≥ 98% on lab set

---

## Phase 2 — Embedding & Matching

- [ ] Integrate ONNX runtime and model loader
- [ ] Implement alignment & preprocessing (112×112 or model input)
- [ ] Implement L2 normalization & cosine similarity function
- [ ] Threshold tuning utility (batch evaluation script)

**Validation Checklist:**
- [ ] EER report generated for target dataset
- [ ] Matching latency measured and within budget

---

## Phase 3 — Liveness Stack (RGB)

- [ ] Blink detection (EAR) module with per-frame EAR computation
- [ ] Optical flow variance test (Lucas-Kanade) across face regions
- [ ] LBP texture classifier trained on printed vs real skin samples
- [ ] Challenge-response optional module (random micro head turn)

**Validation Checklist:**
- [ ] Liveness false acceptance (photo/video) rate validated
- [ ] Liveness returns decision within 500–800ms

---

## Phase 4 — IR Integration (if IR hardware available)

- [ ] IR camera frame capture + synchronised sampling with RGB (if both present)
- [ ] Compute IR reflectance signature (average ROI intensity + variance curve)
- [ ] IR vs display classifier (detect screens/LCD/OLED)
- [ ] IR+RGB fusion rules: if IR available, weight IR signal higher

**Validation Checklist:**
- [ ] IR detector reduces spoofing false accepts by X% (measure)
- [ ] Graceful fallback: IR missing → stricter RGB thresholds

---

## Phase 5 — Template Storage & Crypto

- [ ] Derive device master key (TPM binding preferred)
- [ ] Implement XChaCha20-Poly1305 (libsodium) encrypt/decrypt routines
- [ ] HMAC or AEAD integrity verification
- [ ] Implement per-template versioning and rollback protection

**Validation Checklist:**
- [ ] Templates decrypt only on same device with correct key
- [ ] Tests for tamper detection; corrupt files are rejected

---

## Phase 6 — PAM Module & CLI

- [ ] Implement PAM C ABI wrapper (pam_sm_authenticate, pam_sm_setcred)
- [ ] Implement enrollment CLI with guided multi-angle capture
- [ ] Add configuration options (thresholds, timeout, lockout)
- [ ] Implement fail counters and integration with `pam_faillock.so` if available

**Validation Checklist:**
- [ ] PAM return codes follow libpam contract
- [ ] Failover to `pam_unix.so` confirmed

---

## Phase 7 — Hardening & Testing

- [ ] Memory zeroization for embeddings and secrets (use zeroize)
- [ ] CPU & memory profiling; reduce dynamic dependencies
- [ ] Penetration tests: photo, video, mask, printing, screen attacks
- [ ] Fuzzing for PAM input vectors

**Validation Checklist:**
- [ ] No sensitive data in core dumps or logs
- [ ] Rate limiter & lockout functional

---

## Phase 8 — Ops, Packaging & Deployment

- [ ] Build packages (.deb) with minimal dependencies
- [ ] Systemd service or socket (if needed for enrollment)
- [ ] Packaging includes PAM config snippets and installation script
- [ ] Documentation: admin, enrollment UX, emergency disable switch

**Validation Checklist:**
- [ ] Upgrade path for model & template schema
- [ ] Uninstall removes templates and config safely

---

## Acceptance Metrics (Final Validation)

- [ ] False Accept Rate (FAR) ≤ 0.1%
- [ ] False Reject Rate (FRR) ≤ 2–5%
- [ ] End-to-end auth latency ≤ 2s (median)
- [ ] Template file size ≤ 8KB per user
- [ ] Binary size (core) < 25 MB
- [ ] Max CPU utilization < 30% during active auth
- [ ] Lockout on >5 failed attempts with cooldown 15 min

---

## Testing Plan

### Unit Tests
- [ ] Detector false positives/negatives
- [ ] EAR thresholds across sample frames
- [ ] Encryption/decryption correctness & tamper rejection
- [ ] PAM return codes

### Integration Tests
- [ ] Simulated login flows in VM with GDM
- [ ] Enrollment → delete template → ensure fail
- [ ] IR+RGB fusion path tests (with and without IR)

### Adversarial Tests
- [ ] Photo printed test (high-quality print)
- [ ] Video replay on phone/tablet
- [ ] Mask tests (latex / 3D printed)
- [ ] Screen replay tests (LCD/OLED)

### Metrics to Gather
- [ ] FAR, FRR, EER (ROC curve)
- [ ] Latency distribution (p50, p90, p99)
- [ ] CPU and memory usage during auth
- [ ] Number of false liveness accepts

---

## Pre-Production Checklist

- [ ] Emergency disable mechanism implemented (`/etc/suspend_slfam`)
- [ ] Template migration path for rolling updates
- [ ] Admin documentation complete
- [ ] Security audit completed by third party
- [ ] Legal review for biometric data compliance
- [ ] User consent and deletion mechanisms implemented
- [ ] Backup password recovery tested
- [ ] VM testing completed before production deployment
