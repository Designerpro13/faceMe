//! MobileFaceNet embedding generator

use crate::detection::onnx::{OnnxModel, preprocess_image_arcface};
use crate::detection::AlignedFace;
use crate::error::{EmbeddingError, Result};
use std::path::Path;
use zeroize::Zeroize;

/// Standard embedding dimension for MobileFaceNet
pub const EMBEDDING_DIM: usize = 512;

/// A face embedding vector
#[derive(Clone)]
pub struct FaceEmbedding {
    /// The embedding vector (L2 normalized)
    data: Vec<f32>,
}

impl FaceEmbedding {
    /// Create a new embedding from data
    ///
    /// # Arguments
    ///
    /// * `data` - Raw embedding vector
    /// * `normalize` - Whether to L2 normalize
    pub fn new(mut data: Vec<f32>, normalize: bool) -> Self {
        if normalize {
            super::l2_normalize(&mut data);
        }
        Self { data }
    }

    /// Create an embedding from a slice
    pub fn from_slice(slice: &[f32], normalize: bool) -> Self {
        Self::new(slice.to_vec(), normalize)
    }

    /// Get the embedding data
    #[must_use]
    pub fn data(&self) -> &[f32] {
        &self.data
    }

    /// Get embedding dimension
    #[must_use]
    pub fn dim(&self) -> usize {
        self.data.len()
    }

    /// Compute cosine similarity with another embedding
    #[must_use]
    pub fn similarity(&self, other: &FaceEmbedding) -> f32 {
        super::cosine_similarity(&self.data, &other.data)
    }

    /// Compute Euclidean distance to another embedding
    #[must_use]
    pub fn distance(&self, other: &FaceEmbedding) -> f32 {
        super::euclidean_distance(&self.data, &other.data)
    }

    /// Check if embeddings match above a threshold
    #[must_use]
    pub fn matches(&self, other: &FaceEmbedding, threshold: f32) -> bool {
        self.similarity(other) >= threshold
    }

    /// Convert to bytes for storage
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.data.len() * 4);
        for &v in &self.data {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes
    }

    /// Create from bytes
    ///
    /// # Errors
    ///
    /// Returns an error if data length is invalid
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() % 4 != 0 {
            return Err(EmbeddingError::InvalidDimension {
                expected: EMBEDDING_DIM,
                got: bytes.len() / 4,
            }
            .into());
        }

        let mut data = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks(4) {
            let arr: [u8; 4] = chunk.try_into().map_err(|_| EmbeddingError::InvalidDimension {
                expected: EMBEDDING_DIM,
                got: data.len(),
            })?;
            data.push(f32::from_le_bytes(arr));
        }

        Ok(Self { data })
    }
}

impl Drop for FaceEmbedding {
    fn drop(&mut self) {
        self.data.zeroize();
    }
}

impl std::fmt::Debug for FaceEmbedding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FaceEmbedding")
            .field("dim", &self.data.len())
            .field("norm", &self.data.iter().map(|x| x * x).sum::<f32>().sqrt())
            .finish()
    }
}

/// MobileFaceNet-based embedding generator
pub struct EmbeddingGenerator {
    /// ONNX model
    model: OnnxModel,
    /// Expected input size
    input_size: (usize, usize),
    /// Output dimension
    output_dim: usize,
}

impl EmbeddingGenerator {
    /// Load a MobileFaceNet model
    ///
    /// # Arguments
    ///
    /// * `model_path` - Path to the ONNX model
    ///
    /// # Errors
    ///
    /// Returns an error if the model cannot be loaded
    pub fn load<P: AsRef<Path>>(model_path: P) -> Result<Self> {
        let model = OnnxModel::load(model_path)?;
        
        let input_size = (model.input_width(), model.input_height());
        
        // Determine output dimension from model
        let output_dim = EMBEDDING_DIM; // Default, will be verified on first inference

        Ok(Self {
            model,
            input_size,
            output_dim,
        })
    }

    /// Generate embedding from an aligned face
    ///
    /// # Arguments
    ///
    /// * `face` - Aligned face image (should be 112x112 BGR)
    ///
    /// # Errors
    ///
    /// Returns an error if inference fails
    pub fn generate(&self, face: &AlignedFace) -> Result<FaceEmbedding> {
        self.generate_from_raw(face.data(), face.width(), face.height())
    }

    /// Generate embedding from raw image data
    ///
    /// # Arguments
    ///
    /// * `data` - BGR image data
    /// * `width` - Image width
    /// * `height` - Image height
    ///
    /// # Errors
    ///
    /// Returns an error if inference fails
    pub fn generate_from_raw(&self, data: &[u8], width: u32, height: u32) -> Result<FaceEmbedding> {
        // Preprocess with ArcFace normalization
        let input = preprocess_image_arcface(
            data,
            width as usize,
            height as usize,
            self.input_size.0,
            self.input_size.1,
        );

        // Run inference
        let outputs = self.model.run(input)?;

        // Extract embedding
        if outputs.is_empty() {
            return Err(EmbeddingError::GenerationFailed("No output from model".to_string()).into());
        }

        let embedding_data = outputs[0]
            .as_slice()
            .ok_or_else(|| EmbeddingError::GenerationFailed("Cannot read output tensor".to_string()))?;

        if embedding_data.len() < self.output_dim {
            return Err(EmbeddingError::InvalidDimension {
                expected: self.output_dim,
                got: embedding_data.len(),
            }
            .into());
        }

        // Create normalized embedding
        Ok(FaceEmbedding::new(embedding_data[..self.output_dim].to_vec(), true))
    }

    /// Get expected input size
    #[must_use]
    pub fn input_size(&self) -> (usize, usize) {
        self.input_size
    }

    /// Get output dimension
    #[must_use]
    pub fn output_dim(&self) -> usize {
        self.output_dim
    }
}

impl super::Embedder for EmbeddingGenerator {
    fn generate(&self, face: &AlignedFace) -> Result<FaceEmbedding> {
        EmbeddingGenerator::generate(self, face)
    }

    fn generate_from_raw(&self, data: &[u8], width: u32, height: u32) -> Result<FaceEmbedding> {
        EmbeddingGenerator::generate_from_raw(self, data, width, height)
    }

    fn embedding_dim(&self) -> usize {
        self.output_dim
    }
}

/// Simple embedding generator for testing (deterministic based on image content)
pub struct MockEmbeddingGenerator {
    /// Output dimension
    dim: usize,
}

impl MockEmbeddingGenerator {
    /// Create a new mock generator
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }

    /// Generate a deterministic embedding based on image content
    pub fn generate(&self, data: &[u8]) -> FaceEmbedding {
        let mut embedding = vec![0.0f32; self.dim];

        // Generate embedding based on image statistics
        if !data.is_empty() {
            // Use image content to generate pseudo-embedding
            let mean: f32 = data.iter().map(|&v| v as f32).sum::<f32>() / data.len() as f32;
            
            for (i, e) in embedding.iter_mut().enumerate() {
                // Mix mean with position-dependent noise
                let idx = (i * 17) % data.len();
                let val = data[idx] as f32 / 255.0;
                *e = (mean / 255.0 + val - 0.5) * ((i as f32 + 1.0).sin() * 0.5 + 0.5);
            }
        }

        FaceEmbedding::new(embedding, true)
    }
}

impl Default for MockEmbeddingGenerator {
    fn default() -> Self {
        Self::new(EMBEDDING_DIM)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_creation() {
        let data = vec![0.5; 512];
        let embedding = FaceEmbedding::new(data, true);
        
        assert_eq!(embedding.dim(), 512);
        
        // Should be normalized
        let norm: f32 = embedding.data().iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_embedding_similarity() {
        let a = FaceEmbedding::new(vec![1.0, 0.0, 0.0], true);
        let b = FaceEmbedding::new(vec![1.0, 0.0, 0.0], true);
        
        let sim = a.similarity(&b);
        assert!((sim - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_embedding_to_from_bytes() {
        let original = FaceEmbedding::new(vec![0.1, 0.2, 0.3, 0.4], false);
        let bytes = original.to_bytes();
        let restored = FaceEmbedding::from_bytes(&bytes).unwrap();
        
        assert_eq!(original.dim(), restored.dim());
        for (a, b) in original.data().iter().zip(restored.data().iter()) {
            assert!((a - b).abs() < 0.0001);
        }
    }

    #[test]
    fn test_embedding_matches() {
        let a = FaceEmbedding::new(vec![1.0, 0.0, 0.0, 0.0], true);
        let b = FaceEmbedding::new(vec![0.9, 0.1, 0.0, 0.0], true);
        
        assert!(a.matches(&b, 0.9)); // Similar enough
        assert!(!a.matches(&b, 0.99)); // Threshold too high
    }

    #[test]
    fn test_mock_generator() {
        let gen = MockEmbeddingGenerator::new(128);
        
        // Same input should give same output
        let data = vec![100u8; 1000];
        let e1 = gen.generate(&data);
        let e2 = gen.generate(&data);
        
        assert_eq!(e1.dim(), 128);
        assert!((e1.similarity(&e2) - 1.0).abs() < 0.0001);
        
        // Different input should give different output
        let data2 = vec![200u8; 1000];
        let e3 = gen.generate(&data2);
        assert!(e1.similarity(&e3) < 0.99);
    }

    #[test]
    fn test_embedding_debug() {
        let embedding = FaceEmbedding::new(vec![0.5; 512], true);
        let debug = format!("{:?}", embedding);
        assert!(debug.contains("FaceEmbedding"));
        assert!(debug.contains("dim"));
    }
}
