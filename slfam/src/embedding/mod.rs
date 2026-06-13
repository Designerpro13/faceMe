//! # Face Embedding Generation Module
//!
//! Generates 512-dimensional face embeddings using MobileFaceNet ONNX model.
//! These embeddings can be compared for face verification.

mod mobilefacenet;

pub use mobilefacenet::{EmbeddingGenerator, FaceEmbedding};

use crate::detection::AlignedFace;
use crate::error::Result;

/// Trait for embedding generation
pub trait Embedder: Send + Sync {
    /// Generate embedding from an aligned face
    fn generate(&self, face: &AlignedFace) -> Result<FaceEmbedding>;

    /// Generate embedding from raw BGR image data
    fn generate_from_raw(&self, data: &[u8], width: u32, height: u32) -> Result<FaceEmbedding>;

    /// Get embedding dimension
    fn embedding_dim(&self) -> usize;
}

/// Compare two embeddings using cosine similarity
///
/// # Arguments
///
/// * `a` - First embedding
/// * `b` - Second embedding
///
/// # Returns
///
/// Similarity score in range [-1, 1] where 1 is identical
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for (va, vb) in a.iter().zip(b.iter()) {
        dot += va * vb;
        norm_a += va * va;
        norm_b += vb * vb;
    }

    let denom = (norm_a * norm_b).sqrt();
    if denom > 1e-10 {
        dot / denom
    } else {
        0.0
    }
}

/// Compare two embeddings using Euclidean distance
///
/// # Arguments
///
/// * `a` - First embedding
/// * `b` - Second embedding
///
/// # Returns
///
/// L2 distance (lower is more similar)
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return f32::INFINITY;
    }

    let sum: f32 = a.iter().zip(b.iter()).map(|(va, vb)| (va - vb).powi(2)).sum();
    sum.sqrt()
}

/// L2 normalize a vector in place
pub fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-10 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// L2 normalize a vector, returning a new vector
#[must_use]
pub fn l2_normalized(v: &[f32]) -> Vec<f32> {
    let mut result = v.to_vec();
    l2_normalize(&mut result);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_euclidean_distance() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![3.0, 4.0, 0.0];
        let dist = euclidean_distance(&a, &b);
        assert!((dist - 5.0).abs() < 0.0001);
    }

    #[test]
    fn test_l2_normalize() {
        let mut v = vec![3.0, 4.0];
        l2_normalize(&mut v);
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_l2_normalized() {
        let v = vec![3.0, 4.0];
        let normalized = l2_normalized(&v);
        let norm: f32 = normalized.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_empty_vectors() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        assert!(cosine_similarity(&a, &b).abs() < 0.0001);
    }

    #[test]
    fn test_mismatched_lengths() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < 0.0001);
        assert!(euclidean_distance(&a, &b).is_infinite());
    }
}
