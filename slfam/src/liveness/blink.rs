//! Blink detection using Eye Aspect Ratio (EAR)

use crate::detection::landmarks::{euclidean_distance, FaceLandmarks};

/// Blink detection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlinkState {
    /// Eyes are open
    Open,
    /// Eyes appear to be closing
    Closing,
    /// Eyes are closed (blink in progress)
    Closed,
    /// Eyes are opening (blink completing)
    Opening,
}

/// Blink detector using Eye Aspect Ratio (EAR)
pub struct BlinkDetector {
    /// EAR threshold for closed eyes
    threshold: f32,
    /// Number of consecutive frames needed to confirm state
    consecutive_frames: usize,
    /// Current state
    state: BlinkState,
    /// Number of frames in current state
    frames_in_state: usize,
    /// Number of complete blinks detected
    blink_count: usize,
    /// Recent EAR values
    ear_history: Vec<f32>,
    /// Maximum history size
    max_history: usize,
}

impl BlinkDetector {
    /// Create a new blink detector
    ///
    /// # Arguments
    ///
    /// * `threshold` - EAR threshold below which eyes are considered closed (typically 0.2)
    /// * `consecutive_frames` - Number of frames needed to confirm blink (typically 2-4)
    pub fn new(threshold: f32, consecutive_frames: usize) -> Self {
        Self {
            threshold,
            consecutive_frames,
            state: BlinkState::Open,
            frames_in_state: 0,
            blink_count: 0,
            ear_history: Vec::new(),
            max_history: 30,
        }
    }

    /// Compute Eye Aspect Ratio for both eyes from landmarks
    ///
    /// EAR = (||p2-p6|| + ||p3-p5||) / (2 * ||p1-p4||)
    ///
    /// where p1-p6 are the 6 eye landmark points
    #[must_use]
    pub fn compute_ear(&self, landmarks: &FaceLandmarks) -> f32 {
        let left_ear = self.compute_single_ear(landmarks.left_eye());
        let right_ear = self.compute_single_ear(landmarks.right_eye());

        // Return average of both eyes
        (left_ear + right_ear) / 2.0
    }

    /// Compute EAR for a single eye
    ///
    /// Eye landmarks should be in the order:
    /// 0: left corner
    /// 1: upper-left
    /// 2: upper-right  
    /// 3: right corner
    /// 4: lower-right
    /// 5: lower-left
    fn compute_single_ear(&self, eye: &[(f32, f32)]) -> f32 {
        if eye.len() < 6 {
            return 0.5; // Default to "open" if landmarks missing
        }

        // Vertical distances
        let v1 = euclidean_distance(eye[1], eye[5]); // upper-left to lower-left
        let v2 = euclidean_distance(eye[2], eye[4]); // upper-right to lower-right

        // Horizontal distance
        let h = euclidean_distance(eye[0], eye[3]); // left corner to right corner

        if h < 1e-6 {
            return 0.5; // Avoid division by zero
        }

        (v1 + v2) / (2.0 * h)
    }

    /// Update detector state with new EAR value
    ///
    /// # Arguments
    ///
    /// * `ear` - Current frame's EAR value
    ///
    /// # Returns
    ///
    /// True if a complete blink was just detected
    pub fn update(&mut self, ear: f32) -> bool {
        // Store in history
        self.ear_history.push(ear);
        if self.ear_history.len() > self.max_history {
            self.ear_history.remove(0);
        }

        let is_closed = ear < self.threshold;
        let prev_state = self.state;

        // State machine
        match self.state {
            BlinkState::Open => {
                if is_closed {
                    self.state = BlinkState::Closing;
                    self.frames_in_state = 1;
                }
            }
            BlinkState::Closing => {
                if is_closed {
                    self.frames_in_state += 1;
                    if self.frames_in_state >= self.consecutive_frames {
                        self.state = BlinkState::Closed;
                        self.frames_in_state = 0;
                    }
                } else {
                    // Eyes opened before threshold reached - not a valid blink
                    self.state = BlinkState::Open;
                    self.frames_in_state = 0;
                }
            }
            BlinkState::Closed => {
                if !is_closed {
                    self.state = BlinkState::Opening;
                    self.frames_in_state = 1;
                }
            }
            BlinkState::Opening => {
                if !is_closed {
                    self.frames_in_state += 1;
                    if self.frames_in_state >= self.consecutive_frames {
                        // Complete blink detected
                        self.state = BlinkState::Open;
                        self.frames_in_state = 0;
                        self.blink_count += 1;
                        return true;
                    }
                } else {
                    // Eyes closed again
                    self.state = BlinkState::Closed;
                    self.frames_in_state = 0;
                }
            }
        }

        false
    }

    /// Check if a blink has been detected
    #[must_use]
    pub fn blink_detected(&self) -> bool {
        self.blink_count > 0
    }

    /// Get total number of blinks detected
    #[must_use]
    pub fn blink_count(&self) -> usize {
        self.blink_count
    }

    /// Get current state
    #[must_use]
    pub fn state(&self) -> BlinkState {
        self.state
    }

    /// Get most recent EAR value
    #[must_use]
    pub fn last_ear(&self) -> Option<f32> {
        self.ear_history.last().copied()
    }

    /// Get average EAR over recent history
    #[must_use]
    pub fn average_ear(&self) -> f32 {
        if self.ear_history.is_empty() {
            return 0.5;
        }
        self.ear_history.iter().sum::<f32>() / self.ear_history.len() as f32
    }

    /// Get EAR variance (useful for detecting consistent values from photos)
    #[must_use]
    pub fn ear_variance(&self) -> f32 {
        if self.ear_history.len() < 2 {
            return 0.0;
        }

        let mean = self.average_ear();
        let variance: f32 = self.ear_history
            .iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f32>()
            / self.ear_history.len() as f32;

        variance
    }

    /// Reset detector state
    pub fn reset(&mut self) {
        self.state = BlinkState::Open;
        self.frames_in_state = 0;
        self.blink_count = 0;
        self.ear_history.clear();
    }

    /// Set threshold
    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold.clamp(0.1, 0.4);
    }
}

/// Quick blink detection without full state machine
///
/// Analyzes a sequence of EAR values and returns true if a blink pattern is found
pub fn detect_blink_in_sequence(ear_values: &[f32], threshold: f32, min_closed_frames: usize) -> bool {
    if ear_values.len() < min_closed_frames + 2 {
        return false;
    }

    let mut consecutive_closed = 0;
    let mut was_open_before = false;
    let mut blink_started = false;

    for (i, &ear) in ear_values.iter().enumerate() {
        if ear >= threshold {
            // Eyes open
            if blink_started && consecutive_closed >= min_closed_frames {
                // Completed a blink
                return true;
            }
            was_open_before = true;
            consecutive_closed = 0;
            blink_started = false;
        } else {
            // Eyes closed
            if was_open_before {
                blink_started = true;
            }
            if blink_started {
                consecutive_closed += 1;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_eye_landmarks(ear: f32) -> Vec<(f32, f32)> {
        // Create eye landmarks with a specific EAR
        // Horizontal distance = 20, so vertical should be 2 * ear * 20 = 40 * ear total
        let h = 20.0;
        let v = ear * h; // Each vertical distance

        vec![
            (0.0, 0.0),      // left corner (0)
            (5.0, -v),       // upper-left (1)
            (15.0, -v),      // upper-right (2)
            (20.0, 0.0),     // right corner (3)
            (15.0, v),       // lower-right (4)
            (5.0, v),        // lower-left (5)
        ]
    }

    #[test]
    fn test_ear_computation() {
        let detector = BlinkDetector::new(0.2, 3);

        // Test with open eyes (EAR ≈ 0.3)
        let open_eye = make_eye_landmarks(0.3);
        let ear = detector.compute_single_ear(&open_eye);
        assert!((ear - 0.3).abs() < 0.05);

        // Test with closed eyes (EAR ≈ 0.15)
        let closed_eye = make_eye_landmarks(0.15);
        let ear = detector.compute_single_ear(&closed_eye);
        assert!((ear - 0.15).abs() < 0.05);
    }

    #[test]
    fn test_blink_detection() {
        let mut detector = BlinkDetector::new(0.2, 2);

        // Simulate open eyes
        for _ in 0..5 {
            assert!(!detector.update(0.3));
        }
        assert_eq!(detector.state(), BlinkState::Open);

        // Simulate eyes closing
        assert!(!detector.update(0.15));
        assert_eq!(detector.state(), BlinkState::Closing);
        assert!(!detector.update(0.12));
        // After 2 frames below threshold, should be Closed
        assert_eq!(detector.state(), BlinkState::Closed);

        // Simulate eyes opening
        assert!(!detector.update(0.25));
        assert_eq!(detector.state(), BlinkState::Opening);

        // Complete the blink
        let blink = detector.update(0.28);
        assert!(blink);
        assert!(detector.blink_detected());
        assert_eq!(detector.blink_count(), 1);
    }

    #[test]
    fn test_interrupted_blink() {
        let mut detector = BlinkDetector::new(0.2, 3);

        // Start closing
        detector.update(0.15);
        assert_eq!(detector.state(), BlinkState::Closing);

        // Open before threshold met - should reset
        detector.update(0.3);
        assert_eq!(detector.state(), BlinkState::Open);
        assert!(!detector.blink_detected());
    }

    #[test]
    fn test_ear_variance() {
        let mut detector = BlinkDetector::new(0.2, 2);

        // Add constant values (photo simulation)
        for _ in 0..10 {
            detector.update(0.25);
        }
        let variance_const = detector.ear_variance();

        // Reset and add varying values (real person)
        detector.reset();
        let values = [0.28, 0.26, 0.27, 0.25, 0.29, 0.15, 0.12, 0.24, 0.28, 0.26];
        for v in values {
            detector.update(v);
        }
        let variance_varying = detector.ear_variance();

        // Varying values should have higher variance
        assert!(variance_varying > variance_const);
    }

    #[test]
    fn test_detect_blink_in_sequence() {
        // No blink - constant values
        let constant = vec![0.25; 10];
        assert!(!detect_blink_in_sequence(&constant, 0.2, 2));

        // Valid blink pattern
        let blink = vec![0.28, 0.26, 0.15, 0.12, 0.14, 0.25, 0.28];
        assert!(detect_blink_in_sequence(&blink, 0.2, 2));

        // Incomplete blink (opens too quickly)
        let incomplete = vec![0.28, 0.15, 0.28];
        assert!(!detect_blink_in_sequence(&incomplete, 0.2, 2));
    }

    #[test]
    fn test_reset() {
        let mut detector = BlinkDetector::new(0.2, 2);
        
        // Detect a blink
        for _ in 0..5 { detector.update(0.3); }
        detector.update(0.15);
        detector.update(0.12);
        detector.update(0.25);
        detector.update(0.28);
        
        assert!(detector.blink_detected());
        
        // Reset
        detector.reset();
        assert!(!detector.blink_detected());
        assert_eq!(detector.blink_count(), 0);
        assert_eq!(detector.state(), BlinkState::Open);
    }
}
