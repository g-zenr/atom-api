//! The hash-based embedder: the test double that needs no model file.
//!
//! ADR-0008 §2 puts this implementation behind **every automated test**, so the
//! plumbing — storage, upsert, re-rank, degradation — can be covered without a
//! model artifact and without a network. It is deliberately *not* semantically
//! meaningful: it proves that vectors flow, not that they are good.
//!
//! Construction reads no file and contact no network, which is what the spike
//! asserts.

use sha2::{Digest, Sha256};

use super::embedder::{DEFAULT_DIMENSION, Embedder, EmbedderStatus, MlError};

/// A deterministic, file-free embedder.
pub struct Deterministic {
    model_id: String,
    dimension: usize,
}

impl Deterministic {
    /// Build an embedder that produces `dimension`-element vectors.
    ///
    /// # Panics
    ///
    /// Panics when `dimension` is zero. This is a programming error in the
    /// caller, not a request input: the configuration validator refuses a
    /// non-positive `ml.dimension` before an embedder is built.
    #[must_use]
    pub fn new(dimension: usize) -> Self {
        assert!(dimension > 0, "ml dimension must be positive");
        Self {
            model_id: format!("deterministic-sha256-v1-{dimension}"),
            dimension,
        }
    }

    /// The dimension `ml.dimension` defaults to.
    #[must_use]
    pub fn with_default_dimension() -> Self {
        Self::new(DEFAULT_DIMENSION)
    }
}

impl Default for Deterministic {
    fn default() -> Self {
        Self::with_default_dimension()
    }
}

impl Embedder for Deterministic {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn status(&self) -> EmbedderStatus {
        EmbedderStatus::Ready
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>, MlError> {
        // Counter mode: hash the text with a block counter until the requested
        // length is filled. Four bytes per block, so the vector of a long
        // dimension costs a handful of hashes and stays stable forever.
        let mut vector = Vec::with_capacity(self.dimension);
        let mut counter: u32 = 0;

        while vector.len() < self.dimension {
            let mut hasher = Sha256::new();
            hasher.update(counter.to_le_bytes());
            hasher.update(text.as_bytes());
            let digest = hasher.finalize();

            let chunks = digest.as_slice().as_chunks::<4>().0;
            for chunk in chunks {
                if vector.len() == self.dimension {
                    break;
                }
                let raw = u32::from_le_bytes(*chunk);
                // Map the unsigned value onto [-1, 1).
                vector.push((raw as f32 / u32::MAX as f32) * 2.0 - 1.0);
            }

            counter = counter.wrapping_add(1);
        }

        // Unit length, so a cosine score is a plain dot product downstream.
        let norm = super::embedder::l2_norm(&vector);
        if norm > 0.0 {
            let scale = 1.0 / norm as f32;
            for value in &mut vector {
                *value *= scale;
            }
        }

        Ok(vector)
    }
}
