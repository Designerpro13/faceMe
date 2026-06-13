//! # Matching Engine
//!
//! Compares face embeddings and makes authentication decisions.

use crate::config::MatchingConfig;
use crate::embedding::{cosine_similarity, FaceEmbedding};
use crate::error::{MatchingError, Result};
use std::time::{Duration, Instant};

/// Result of a matching operation
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// Whether the match was successful
    pub matched: bool,
    /// Similarity score (0.0 to 1.0)
    pub similarity: f32,
    /// Threshold that was used
    pub threshold: f32,
    /// Time taken for matching
    pub duration: Duration,
    /// Additional details
    pub details: MatchDetails,
}

/// Additional match details
#[derive(Debug, Clone)]
pub struct MatchDetails {
    /// Index of best matching template (if multiple)
    pub best_template_index: Option<usize>,
    /// All similarity scores (if multiple templates)
    pub all_scores: Vec<f32>,
    /// Security level used
    pub security_level: SecurityLevel,
}

/// Security level for matching
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityLevel {
    /// Normal security (standard threshold)
    Normal,
    /// High security (stricter threshold)
    High,
}

impl Default for SecurityLevel {
    fn default() -> Self {
        Self::Normal
    }
}

/// Face matching engine
pub struct Matcher {
    /// Configuration
    config: MatchingConfig,
    /// Current security level
    security_level: SecurityLevel,
}

impl Matcher {
    /// Create a new matcher with configuration
    pub fn new(config: MatchingConfig) -> Self {
        Self {
            config,
            security_level: SecurityLevel::Normal,
        }
    }

    /// Create with default configuration
    pub fn default_config() -> Self {
        Self::new(MatchingConfig::default())
    }

    /// Set security level
    pub fn set_security_level(&mut self, level: SecurityLevel) {
        self.security_level = level;
    }

    /// Get current threshold based on security level
    #[must_use]
    pub fn current_threshold(&self) -> f32 {
        match self.security_level {
            SecurityLevel::Normal => self.config.normal_threshold,
            SecurityLevel::High => self.config.high_security_threshold,
        }
    }

    /// Match a probe embedding against a single enrolled embedding
    ///
    /// # Arguments
    ///
    /// * `probe` - The embedding to verify
    /// * `enrolled` - The enrolled template embedding
    ///
    /// # Returns
    ///
    /// Match result with similarity score
    pub fn match_one(&self, probe: &FaceEmbedding, enrolled: &FaceEmbedding) -> MatchResult {
        let start = Instant::now();
        let threshold = self.current_threshold();

        let similarity = probe.similarity(enrolled);
        let matched = similarity >= threshold;

        MatchResult {
            matched,
            similarity,
            threshold,
            duration: start.elapsed(),
            details: MatchDetails {
                best_template_index: Some(0),
                all_scores: vec![similarity],
                security_level: self.security_level,
            },
        }
    }

    /// Match a probe embedding against multiple enrolled embeddings
    ///
    /// Uses the configured fusion strategy (max, average, etc.)
    ///
    /// # Arguments
    ///
    /// * `probe` - The embedding to verify
    /// * `enrolled` - List of enrolled template embeddings
    ///
    /// # Errors
    ///
    /// Returns an error if no enrolled templates provided
    pub fn match_many(
        &self,
        probe: &FaceEmbedding,
        enrolled: &[FaceEmbedding],
    ) -> Result<MatchResult> {
        if enrolled.is_empty() {
            return Err(MatchingError::NoTemplates.into());
        }

        let start = Instant::now();
        let threshold = self.current_threshold();

        // Compute similarities with all enrolled embeddings
        let scores: Vec<f32> = enrolled.iter().map(|e| probe.similarity(e)).collect();

        // Find best match
        let (best_idx, &best_score) = scores
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap();

        // Apply fusion strategy
        let final_score = match self.config.fusion_strategy.as_str() {
            "max" => best_score,
            "average" => scores.iter().sum::<f32>() / scores.len() as f32,
            "weighted" => {
                // Weight by position (more recent templates get higher weight)
                let weights: Vec<f32> = (0..scores.len())
                    .map(|i| 1.0 + (i as f32 * 0.1))
                    .collect();
                let weighted_sum: f32 = scores.iter().zip(&weights).map(|(s, w)| s * w).sum();
                let weight_sum: f32 = weights.iter().sum();
                weighted_sum / weight_sum
            }
            _ => best_score, // Default to max
        };

        let matched = final_score >= threshold;

        Ok(MatchResult {
            matched,
            similarity: final_score,
            threshold,
            duration: start.elapsed(),
            details: MatchDetails {
                best_template_index: Some(best_idx),
                all_scores: scores,
                security_level: self.security_level,
            },
        })
    }

    /// Verify with quality checks
    ///
    /// Performs additional validation beyond simple threshold matching
    pub fn verify_with_checks(
        &self,
        probe: &FaceEmbedding,
        enrolled: &[FaceEmbedding],
        quality_score: f32,
    ) -> Result<MatchResult> {
        // Check embedding quality
        if quality_score < self.config.min_quality_score {
            return Err(MatchingError::LowQuality {
                score: quality_score,
                threshold: self.config.min_quality_score,
            }
            .into());
        }

        // Check embedding validity (non-zero norm)
        let probe_norm: f32 = probe.data().iter().map(|x| x * x).sum::<f32>().sqrt();
        if probe_norm < 0.1 {
            return Err(MatchingError::InvalidEmbedding("Zero or near-zero norm".to_string()).into());
        }

        self.match_many(probe, enrolled)
    }
}

impl Default for Matcher {
    fn default() -> Self {
        Self::default_config()
    }
}

/// Batch matching for identification (1:N)
pub struct IdentificationMatcher {
    /// Base matcher
    matcher: Matcher,
    /// Maximum results to return
    max_results: usize,
}

impl IdentificationMatcher {
    /// Create a new identification matcher
    pub fn new(config: MatchingConfig, max_results: usize) -> Self {
        Self {
            matcher: Matcher::new(config),
            max_results,
        }
    }

    /// Search for matching identities
    ///
    /// # Arguments
    ///
    /// * `probe` - The embedding to identify
    /// * `gallery` - List of (identity, embeddings) pairs
    ///
    /// # Returns
    ///
    /// List of potential matches sorted by similarity
    pub fn identify<'a>(
        &self,
        probe: &FaceEmbedding,
        gallery: &'a [(String, Vec<FaceEmbedding>)],
    ) -> Vec<IdentificationResult<'a>> {
        let threshold = self.matcher.current_threshold();
        
        let mut results: Vec<IdentificationResult> = gallery
            .iter()
            .filter_map(|(identity, embeddings)| {
                if embeddings.is_empty() {
                    return None;
                }

                let best_score = embeddings
                    .iter()
                    .map(|e| probe.similarity(e))
                    .fold(f32::NEG_INFINITY, f32::max);

                if best_score >= threshold {
                    Some(IdentificationResult {
                        identity,
                        similarity: best_score,
                    })
                } else {
                    None
                }
            })
            .collect();

        // Sort by similarity (descending)
        results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap());

        // Limit results
        results.truncate(self.max_results);

        results
    }
}

/// Result of identification search
#[derive(Debug)]
pub struct IdentificationResult<'a> {
    /// Matched identity
    pub identity: &'a str,
    /// Similarity score
    pub similarity: f32,
}

/// Threshold calibration utilities
pub mod calibration {
    use super::*;

    /// Compute EER (Equal Error Rate) threshold
    ///
    /// # Arguments
    ///
    /// * `genuine_scores` - Similarity scores from genuine pairs
    /// * `impostor_scores` - Similarity scores from impostor pairs
    ///
    /// # Returns
    ///
    /// Threshold at which FAR ≈ FRR
    pub fn compute_eer_threshold(genuine_scores: &[f32], impostor_scores: &[f32]) -> f32 {
        if genuine_scores.is_empty() || impostor_scores.is_empty() {
            return 0.5;
        }

        let mut best_threshold = 0.5;
        let mut min_diff = f32::MAX;

        // Search for threshold where FAR ≈ FRR
        for t in (0..100).map(|i| i as f32 / 100.0) {
            let far = impostor_scores.iter().filter(|&&s| s >= t).count() as f32
                / impostor_scores.len() as f32;
            let frr = genuine_scores.iter().filter(|&&s| s < t).count() as f32
                / genuine_scores.len() as f32;

            let diff = (far - frr).abs();
            if diff < min_diff {
                min_diff = diff;
                best_threshold = t;
            }
        }

        best_threshold
    }

    /// Compute threshold for target FAR
    pub fn threshold_for_far(impostor_scores: &[f32], target_far: f32) -> f32 {
        if impostor_scores.is_empty() {
            return 0.5;
        }

        let mut sorted = impostor_scores.to_vec();
        sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());

        let idx = ((1.0 - target_far) * sorted.len() as f32) as usize;
        sorted.get(idx.min(sorted.len() - 1)).copied().unwrap_or(0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_embedding(values: &[f32]) -> FaceEmbedding {
        FaceEmbedding::from_slice(values, true)
    }

    #[test]
    fn test_match_one_identical() {
        let matcher = Matcher::default_config();
        let e1 = make_embedding(&[1.0, 0.0, 0.0, 0.0]);
        let e2 = make_embedding(&[1.0, 0.0, 0.0, 0.0]);

        let result = matcher.match_one(&e1, &e2);
        assert!(result.matched);
        assert!((result.similarity - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_match_one_different() {
        let matcher = Matcher::default_config();
        let e1 = make_embedding(&[1.0, 0.0, 0.0, 0.0]);
        let e2 = make_embedding(&[0.0, 1.0, 0.0, 0.0]);

        let result = matcher.match_one(&e1, &e2);
        assert!(!result.matched);
        assert!(result.similarity < 0.1);
    }

    #[test]
    fn test_match_many() {
        let matcher = Matcher::default_config();
        let probe = make_embedding(&[1.0, 0.1, 0.0, 0.0]);
        let enrolled = vec![
            make_embedding(&[0.0, 1.0, 0.0, 0.0]),
            make_embedding(&[1.0, 0.0, 0.0, 0.0]),
            make_embedding(&[0.5, 0.5, 0.0, 0.0]),
        ];

        let result = matcher.match_many(&probe, &enrolled).unwrap();
        
        // Should match second enrolled (closest to probe)
        assert_eq!(result.details.best_template_index, Some(1));
        assert_eq!(result.details.all_scores.len(), 3);
    }

    #[test]
    fn test_match_many_no_templates() {
        let matcher = Matcher::default_config();
        let probe = make_embedding(&[1.0, 0.0, 0.0, 0.0]);

        let result = matcher.match_many(&probe, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_security_levels() {
        let mut matcher = Matcher::default_config();
        
        let normal_threshold = matcher.current_threshold();
        
        matcher.set_security_level(SecurityLevel::High);
        let high_threshold = matcher.current_threshold();
        
        assert!(high_threshold > normal_threshold);
    }

    #[test]
    fn test_identification() {
        let config = MatchingConfig {
            normal_threshold: 0.5,
            ..Default::default()
        };
        let id_matcher = IdentificationMatcher::new(config, 3);
        
        let probe = make_embedding(&[1.0, 0.0, 0.0, 0.0]);
        
        let gallery = vec![
            ("alice".to_string(), vec![make_embedding(&[1.0, 0.1, 0.0, 0.0])]),
            ("bob".to_string(), vec![make_embedding(&[0.0, 1.0, 0.0, 0.0])]),
            ("charlie".to_string(), vec![make_embedding(&[0.9, 0.1, 0.0, 0.0])]),
        ];

        let results = id_matcher.identify(&probe, &gallery);
        
        // Alice and Charlie should match, Bob shouldn't
        assert!(results.len() >= 1);
        assert!(results.iter().any(|r| r.identity == "alice"));
    }

    #[test]
    fn test_eer_threshold() {
        let genuine = vec![0.8, 0.85, 0.9, 0.75, 0.82];
        let impostor = vec![0.2, 0.15, 0.3, 0.25, 0.1];

        let threshold = calibration::compute_eer_threshold(&genuine, &impostor);
        
        // Should be somewhere between genuine and impostor distributions
        assert!(threshold > 0.3);
        assert!(threshold < 0.75);
    }

    #[test]
    fn test_threshold_for_far() {
        let impostor = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];
        
        // FAR of 0.1 means 10% of impostors should be accepted
        let threshold = calibration::threshold_for_far(&impostor, 0.1);
        
        // Threshold should be high enough to reject most impostors
        assert!(threshold > 0.8);
    }
}
