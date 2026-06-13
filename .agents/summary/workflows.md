# Workflows — SLFAM

## Authentication Flow

Triggered when PAM calls `pam_sm_authenticate`. Entire flow must complete within `timeout_secs` (default: 10s).

```mermaid
sequenceDiagram
    participant OS as PAM Runtime
    participant PM as pam_sm_authenticate
    participant RL as RateLimiter
    participant PH as PamHandler
    participant CAM as Camera
    participant DET as FaceDetectionPipeline
    participant LIV as LivenessAnalyzer
    participant EMB as EmbeddingGenerator
    participant TPL as TemplateStore
    participant CRY as crypto::decrypt
    participant MAT as Matcher

    OS->>PM: pam_sm_authenticate(pamh, flags, args)
    PM->>PM: parse PamArgs from argv
    PM->>PM: Config::load_or_default()
    PM->>PH: PamHandler::new(config, args)
    PH->>RL: check(username)
    RL-->>PH: Ok (or RateLimited error)
    PH->>PH: initialize() — lazy load TemplateStore + KeyDerivation
    PH->>TPL: load(username, derived_key)
    TPL->>CRY: decrypt(ciphertext, key, aad=username)
    CRY-->>TPL: plaintext Template
    TPL-->>PH: Template {embeddings, metadata}

    loop Capture frames (up to N attempts)
        PH->>CAM: capture_frame()
        CAM-->>PH: Frame
        PH->>DET: process_frame(frame)
        DET-->>PH: ProcessedFace {aligned, landmarks}
        PH->>LIV: add_frame(frame, landmarks)
    end

    PH->>LIV: analyze()
    LIV-->>PH: LivenessResult

    alt Liveness failed
        PH-->>PM: Err(LivenessError)
        PM-->>OS: PAM_AUTH_ERR
    else Liveness passed
        PH->>EMB: generate(aligned_face)
        EMB-->>PH: FaceEmbedding (512D)
        PH->>MAT: match_one(probe, template.embeddings)
        MAT-->>PH: MatchResult

        alt Match succeeded
            PH->>TPL: update metadata (record_auth)
            RL->>RL: clear(username)
            PH-->>PM: Ok(())
            PM-->>OS: PAM_SUCCESS
        else Match failed
            RL->>RL: record_attempt(username)
            PH-->>PM: Err(AuthenticationFailed)
            PM-->>OS: PAM_AUTH_ERR
        end
    end
```

### Error / Fallback Handling

```mermaid
flowchart TD
    E[AuthError] --> SF{should_fallback?}
    SF -- Yes\n(camera, hardware) --> FB[Return PAM_AUTHINFO_UNAVAIL\nPAM tries next module\n→ password fallback]
    SF -- No --> SC{is_security_concern?}
    SC -- Yes\n(crypto, template tamper) --> LOG[Log security event\nReturn PAM_AUTH_ERR]
    SC -- No --> FAIL[Return PAM_AUTH_ERR]
```

---

## Enrollment Flow (`slfam-enroll`)

```mermaid
sequenceDiagram
    participant CLI as slfam-enroll
    participant CFG as Config
    participant CAM as Camera
    participant DET as FaceDetectionPipeline
    participant EMB as EmbeddingGenerator
    participant CRY as KeyDerivation
    participant TPL as TemplateStore

    CLI->>CFG: Config::load(config_path)
    CLI->>CRY: TpmKeyDerivation::new(key_path, use_tpm)
    CLI->>CAM: open_default() or open_path()
    CLI->>DET: FaceDetectionPipeline::new(config)
    CLI->>EMB: EmbeddingGenerator::load(model_dir)

    CLI->>TPL: TemplateStore::new(template_dir)
    
    loop For each sample (default: 5)
        CLI->>CLI: Prompt user "Look at camera..."
        CLI->>CAM: capture_frame()
        CAM-->>CLI: Frame
        CLI->>DET: process_frame(frame)
        DET-->>CLI: ProcessedFace
        CLI->>EMB: generate(aligned_face)
        EMB-->>CLI: FaceEmbedding
        CLI->>CLI: template.add_embedding(embedding)
        CLI->>CLI: Show progress
    end

    CLI->>CRY: derive_key(username, context)
    CRY-->>CLI: DerivedKey
    CLI->>TPL: save(username, template, key)
    TPL->>CRY: encrypt(serialized_template, key, aad=username)
    CRY-->>TPL: EncryptedData
    TPL-->>CLI: Ok(())
    CLI->>CLI: Print "Enrollment complete"
```

---

## Liveness Detection Sub-Pipeline

Runs across a sequence of frames (typically 5–10) captured over ~1–2 seconds.

```mermaid
flowchart TD
    Start([add_frame called N times]) --> BL[BlinkDetector\nUpdate EAR state machine]
    Start --> OF[OpticalFlowAnalyzer\nCompute SAD block motion]
    Start --> LBP[LbpTextureAnalyzer\nCompute LBP histograms]
    Start --> IR[IrReflectanceAnalyzer\nAnalyze IR pixel range]

    BL --> BLC{Blink\ndetected?}
    OF --> OFC{Natural\nmotion?}
    LBP --> LBPC{Texture\nlooks real?}
    IR --> IRC{Reflectance\nin range?}

    BLC -- require_blink=true --> AND
    OFC --> AND
    LBPC --> AND
    IRC -- enable_ir=true --> AND

    AND{All enabled\nchecks pass?} -- Yes --> PASS[LivenessResult::passed=true]
    AND -- No --> FAIL[LivenessResult::passed=false\nwith per-check reasons]
```

### Blink Detection Detail (EAR)

Eye Aspect Ratio (EAR) = (vertical distances) / (2 × horizontal distance)

```mermaid
stateDiagram-v2
    [*] --> Open
    Open --> Closing : EAR < threshold
    Closing --> Closed : EAR < min_ear
    Closed --> Opening : EAR > threshold
    Opening --> Open : EAR > open_threshold
    Opening --> BlinkDetected : transition complete
    BlinkDetected --> Open
```

---

## Template Encryption/Decryption Flow

```mermaid
flowchart LR
    subgraph Save
        T[Template struct] -->|JSON serialize| J[JSON bytes]
        J -->|XChaCha20-Poly1305\nAAD = user_id| E[EncryptedData]
        E -->|to_bytes| B[b'SLFM' + v1 + bytes]
        B -->|write| F[user.slfm file]
    end

    subgraph Load
        F2[user.slfm file] -->|read + verify magic| E2[EncryptedData]
        E2 -->|decrypt\nAAD = user_id| J2[JSON bytes]
        J2 -->|deserialize| T2[Template struct]
    end
```

Key derivation for the encryption key:

```mermaid
flowchart TD
    U[username + context] --> KD[KeyDerivation trait]
    KD --> TPM{TPM\navailable?}
    TPM -- Yes --> TK[TPM-sealed key]
    TPM -- No --> MK[Argon2id from\nmachine-id + salt\nstored at .key]
    TK --> DK[DerivedKey 32 bytes]
    MK --> DK
```

---

## Rate Limiting Flow

```mermaid
flowchart TD
    A[Authentication attempt] --> B[RateLimiter::check]
    B --> C{attempts >=\nmax_attempts?}
    C -- Yes --> D{lockout\nexpired?}
    D -- No --> E[Return RateLimited\nwith remaining_secs]
    D -- Yes --> F[Reset counter\nAllow attempt]
    C -- No --> F
    F --> G[... run auth ...]
    G --> H{Success?}
    H -- Yes --> I[RateLimiter::clear\nreset counter]
    H -- No --> J[RateLimiter::record_attempt\nincrement counter]
```

---

## Camera Device Discovery

```mermaid
flowchart TD
    Start([Camera::open]) --> AD{auto_detect\nconfigured?}
    AD -- Yes --> ENUM[enumerate_cameras\n/dev/video0..N]
    AD -- No --> DIRECT[open configured\ndevice_id]
    ENUM --> FINDRGB[find_rgb_camera\ncheck driver name, caps]
    FINDRGB --> FINDIR{prefer_ir\n& IR wanted?}
    FINDIR -- Yes --> FINDIRC[find_ir_camera\ncheck driver keywords]
    FINDIR -- No --> USE[Use RGB camera]
    FINDIRC --> USE
    USE --> LOCK[Acquire DeviceLock\n/tmp/slfam-video{N}.lock]
    LOCK --> MMAP[init_mmap\nV4L2 buffer ring]
    MMAP --> STREAM[start_streaming]
```
