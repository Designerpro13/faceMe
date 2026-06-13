#!/usr/bin/env bash
# setup_models.sh — Download SLFAM ONNX models from InsightFace buffalo_sc pack
# Run as root: sudo bash setup_models.sh
# Total download: ~16 MB

set -euo pipefail

MODEL_DIR="/usr/share/slfam/models"
TMP_DIR="$(mktemp -d)"
PACK_URL="https://github.com/deepinsight/insightface/releases/download/v0.7/buffalo_sc.zip"
PACK_ZIP="$TMP_DIR/buffalo_sc.zip"

cleanup() { rm -rf "$TMP_DIR"; }
trap cleanup EXIT

echo "==> Creating model directory: $MODEL_DIR"
mkdir -p "$MODEL_DIR"

echo "==> Downloading buffalo_sc pack (~16 MB)..."
curl -L --progress-bar -o "$PACK_ZIP" "$PACK_URL"

echo "==> Extracting..."
unzip -q "$PACK_ZIP" -d "$TMP_DIR/buffalo_sc"

# buffalo_sc contains:
#   det_500m.onnx   — SCRFD-500MF face detector
#   w600k_mbf.onnx  — MobileFaceNet embedding model (512D)
# (no landmark or attribute models in the sc pack)

echo "==> Installing models..."
cp "$TMP_DIR/buffalo_sc/buffalo_sc/det_500m.onnx"  "$MODEL_DIR/retinaface.onnx"
cp "$TMP_DIR/buffalo_sc/buffalo_sc/w600k_mbf.onnx" "$MODEL_DIR/mobilefacenet.onnx"

# Landmark model: buffalo_sc doesn't include one.
# Download the 2d106det model from the buffalo_l pack (landmark only).
echo "==> Downloading landmark model from buffalo_l (~few MB)..."
LMRK_URL="https://github.com/deepinsight/insightface/releases/download/v0.7/buffalo_l.zip"
LMRK_ZIP="$TMP_DIR/buffalo_l.zip"
curl -L --progress-bar -o "$LMRK_ZIP" "$LMRK_URL"
unzip -q "$LMRK_ZIP" -d "$TMP_DIR/buffalo_l"
cp "$TMP_DIR/buffalo_l/buffalo_l/2d106det.onnx" "$MODEL_DIR/landmark_106_2d.onnx"

# Set permissions
chmod 644 "$MODEL_DIR"/*.onnx

echo ""
echo "==> Done. Models installed:"
ls -lh "$MODEL_DIR"
echo ""
echo "NOTE: Update your config.toml — the model filenames are:"
echo "  [detection]"
echo "  detection_model = \"retinaface.onnx\"    # SCRFD-500MF"
echo "  landmark_model  = \"landmark_106_2d.onnx\" # 106-point 2D"
echo "  embedding_model = \"mobilefacenet.onnx\"  # w600k_mbf 512D"
echo ""
echo "  [matching]"
echo "  embedding_model = \"mobilefacenet.onnx\""
echo ""
echo "  [liveness]"
echo "  enable_lbp = false  # no LBP model needed (using texture analysis only)"
