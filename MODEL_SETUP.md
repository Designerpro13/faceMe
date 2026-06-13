# SLFAM Model Setup Guide

This document describes the ONNX models required for SLFAM and how to obtain them.

---

## Required Models

SLFAM requires 4 ONNX models for full functionality:

1. **Face Detection** - RetinaFace or similar
2. **Facial Landmarks** - 68-point detector
3. **Face Embedding** - MobileFaceNet or ArcFace
4. **LBP Classifier** - Texture analysis for liveness

---

## Model Directory Structure

```
/usr/share/slfam/models/
├── retinaface.onnx          # Face detection
├── landmarks_68.onnx        # Landmark detection
├── mobilefacenet.onnx       # Face embedding
└── lbp_classifier.onnx      # Liveness texture classifier
```

---

## 1. Face Detection Model

### RetinaFace

**Purpose:** Detect faces in camera frames 
**Input:** RGB image (variable size) 
**Output:** Bounding boxes + confidence scores 
**Size:** ~5-10 MB

**Option A: ONNX Model Zoo**
```bash
# Download from ONNX Model Zoo
wget https://github.com/onnx/models/raw/main/vision/body_analysis/retinaface/retinaface.onnx \
  -O /usr/share/slfam/models/retinaface.onnx
```

**Option B: Convert from PyTorch**
```python
# If you have a PyTorch RetinaFace model
import torch
import onnx

model = torch.load('retinaface_pytorch.pth')
model.eval()

dummy_input = torch.randn(1, 3, 640, 640)
torch.onnx.export(
    model,
    dummy_input,
    "retinaface.onnx",
    input_names=['input'],
    output_names=['output'],
    dynamic_axes={'input': {0: 'batch', 2: 'height', 3: 'width'}}
)
```

**Alternative Models:**
- SCRFD (lightweight, fast)
- YuNet (OpenCV DNN)
- MediaPipe Face Detection

---

## 2. Facial Landmarks Model

### 68-Point Landmark Detector

**Purpose:** Detect facial keypoints for alignment and liveness  
**Input:** Cropped face image (typically 112x112 or 224x224)  
**Output:** 68 (x,y) coordinates  
**Size:** ~2-5 MB

**Option A: 2D-FAN (Face Alignment Network)**
```bash
# Download pre-trained 2D-FAN model
# Source: https://github.com/1adrianb/face-alignment

# Convert to ONNX (requires face-alignment library)
pip install face-alignment
python convert_landmarks_to_onnx.py
```

**Option B: MediaPipe Face Mesh (simplified)**
```bash
# MediaPipe provides 468 landmarks, can be reduced to 68
# Download from MediaPipe model repository
wget https://storage.googleapis.com/mediapipe-models/face_landmarker/face_landmarker/float16/latest/face_landmarker.task
```

**Option C: Dlib to ONNX**
```python
# Convert dlib's shape predictor to ONNX
import dlib
import onnx
# Note: Requires custom conversion script
```

**Recommended:** Use a lightweight CNN-based 68-point detector trained on 300W dataset.

---

## 3. Face Embedding Model

### MobileFaceNet

**Purpose:** Generate face embeddings for matching  
**Input:** Aligned face image (112x112)  
**Output:** 512-dimensional embedding vector  
**Size:** ~4-6 MB

**Option A: InsightFace MobileFaceNet**
```bash
# Download from InsightFace model zoo
# https://github.com/deepinsight/insightface/tree/master/model_zoo

wget https://github.com/deepinsight/insightface/releases/download/v0.7/mobilefacenet.onnx \
  -O /usr/share/slfam/models/mobilefacenet.onnx
```

**Option B: ArcFace (more accurate, larger)**
```bash
# ArcFace ResNet50 (if you need higher accuracy)
wget https://github.com/deepinsight/insightface/releases/download/v0.7/arcface_r50.onnx \
  -O /usr/share/slfam/models/arcface_r50.onnx

# Update config.toml:
# embedding_dim = 512
```

**Model Requirements:**
- Input: 112x112 RGB image, normalized to [-1, 1] or [0, 1]
- Output: L2-normalized embedding vector
- Trained on large face dataset (MS1MV2, CASIA-WebFace, etc.)

---

## 4. LBP Texture Classifier

### Liveness Detection Classifier

**Purpose:** Distinguish real faces from photos/screens  
**Input:** LBP histogram features (59-dimensional)  
**Output:** Binary classification (real/fake)  
**Size:** ~1 MB

**Training Required:**

This model needs to be trained on your own dataset or obtained from research.

**Option A: Train Your Own**

```python
# train_lbp_classifier.py
import numpy as np
from sklearn.svm import SVC
from sklearn.model_selection import train_test_split
import skl2onnx
from skl2onnx import convert_sklearn
from skl2onnx.common.data_types import FloatTensorType

# Load your dataset
# X_real: LBP histograms from real faces
# X_fake: LBP histograms from photos/screens
X = np.vstack([X_real, X_fake])
y = np.hstack([np.ones(len(X_real)), np.zeros(len(X_fake))])

X_train, X_test, y_train, y_test = train_test_split(X, y, test_size=0.2)

# Train SVM classifier
clf = SVC(kernel='rbf', probability=True)
clf.fit(X_train, y_train)

# Convert to ONNX
initial_type = [('float_input', FloatTensorType([None, 59]))]
onnx_model = convert_sklearn(clf, initial_types=initial_type)

with open("lbp_classifier.onnx", "wb") as f:
    f.write(onnx_model.SerializeToString())
```

**Option B: Use Pre-trained (if available)**

Check academic repositories or anti-spoofing datasets:
- NUAA Photograph Imposter Database
- CASIA Face Anti-Spoofing Database
- Replay-Attack Database

**Option C: Disable LBP (fallback)**

If you can't train/obtain this model:
```toml
# In config.toml
[liveness]
enable_lbp = false
```

---

## Model Verification

After downloading models, verify they work:

```bash
# Create test script
cat > test_models.py << 'EOF'
import onnxruntime as ort
import numpy as np

def test_model(path, input_shape):
    session = ort.InferenceSession(path)
    print(f"\nTesting: {path}")
    print(f"Inputs: {session.get_inputs()[0].name}, shape: {session.get_inputs()[0].shape}")
    print(f"Outputs: {session.get_outputs()[0].name}, shape: {session.get_outputs()[0].shape}")
    
    # Test inference
    dummy_input = np.random.randn(*input_shape).astype(np.float32)
    output = session.run(None, {session.get_inputs()[0].name: dummy_input})
    print(f"Inference successful! Output shape: {output[0].shape}")

# Test each model
test_model("/usr/share/slfam/models/retinaface.onnx", (1, 3, 640, 640))
test_model("/usr/share/slfam/models/landmarks_68.onnx", (1, 3, 112, 112))
test_model("/usr/share/slfam/models/mobilefacenet.onnx", (1, 3, 112, 112))
test_model("/usr/share/slfam/models/lbp_classifier.onnx", (1, 59))
EOF

python3 test_models.py
```

---

## Quick Setup Script

```bash
#!/bin/bash
# setup_models.sh

set -e

MODEL_DIR="/usr/share/slfam/models"
sudo mkdir -p "$MODEL_DIR"

echo "Downloading SLFAM models..."

# Face Detection (RetinaFace)
echo "1/4 Downloading face detection model..."
wget -q --show-progress \
  https://github.com/onnx/models/raw/main/vision/body_analysis/retinaface/retinaface.onnx \
  -O "$MODEL_DIR/retinaface.onnx"

# Landmarks (placeholder - you need to provide this)
echo "2/4 Landmarks model - MANUAL SETUP REQUIRED"
echo "    Please download landmarks_68.onnx and place in $MODEL_DIR"

# Face Embedding (MobileFaceNet from InsightFace)
echo "3/4 Downloading face embedding model..."
wget -q --show-progress \
  https://github.com/deepinsight/insightface/releases/download/v0.7/mobilefacenet.onnx \
  -O "$MODEL_DIR/mobilefacenet.onnx"

# LBP Classifier (needs training)
echo "4/4 LBP classifier - TRAINING REQUIRED"
echo "    See Model Setup Guide for training instructions"
echo "    Or disable in config: enable_lbp = false"

echo ""
echo "Model setup complete!"
echo "Please verify models with: python3 test_models.py"
```

---

## Model Licensing

**Important:** Ensure you have the right to use these models:

- **RetinaFace:** Check original repository license
- **InsightFace models:** Non-commercial use allowed, check for commercial
- **MediaPipe:** Apache 2.0 license
- **Custom trained models:** Your own license

Always review model licenses before deployment!

---

## Performance Considerations

### Model Size vs Accuracy Trade-offs

| Model Type | Size | Accuracy | Speed |
|------------|------|----------|-------|
| MobileFaceNet | 5MB | Good | Fast |
| ArcFace ResNet50 | 166MB | Excellent | Slower |
| RetinaFace | 5MB | Good | Fast |
| SCRFD | 2MB | Good | Very Fast |

### Optimization

For production, consider:
- Quantization (FP16 or INT8) for smaller size
- Model pruning for faster inference
- Hardware acceleration (OpenVINO, TensorRT)

---

## Troubleshooting

### Model Not Loading
```
Error: Failed to load ONNX model
```
**Solution:** Verify ONNX Runtime version compatibility
```bash
pip install onnxruntime --upgrade
```

### Wrong Input Shape
```
Error: Input shape mismatch
```
**Solution:** Check model input requirements with:
```python
import onnx
model = onnx.load("model.onnx")
print(model.graph.input)
```

### Poor Accuracy
- Ensure models are trained on diverse datasets
- Check preprocessing matches training (normalization, color space)
- Verify model quantization didn't degrade quality

---

## Next Steps

After setting up models:
1. Update `config.toml` with correct model filenames
2. Run model verification script
3. Test enrollment: `slfam-enroll --user testuser`
4. Test authentication in dev mode
5. Profile performance and adjust thresholds

---

## References

- ONNX Model Zoo: https://github.com/onnx/models
- InsightFace: https://github.com/deepinsight/insightface
- Face Alignment: https://github.com/1adrianb/face-alignment
- MediaPipe: https://developers.google.com/mediapipe
- Anti-Spoofing Datasets: https://sites.google.com/qq.com/face-anti-spoofing
