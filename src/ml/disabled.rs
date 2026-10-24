//! The embedder that reports unavailability.
//!
//! The default, and the state of any deployment without a model file.
//! ADR-0008 §2 requires that it **never panics and never blocks a request**: it
//! answers `status()` with a reason and `embed()` with an error, so the service
//! can degrade a search to lexical results with `200`.

use super::embedder::{Embedder, EmbedderStatus, MlError};

/// An embedder that cannot produce vectors.
#[derive(Debug, Clone)]
pub struct Disabled {
    model_id: String,
    dimension: usize,
    reason: String,
}

impl Disabled {
    /// Build a disabled embedder that explains itself with `reason`.
    #[must_use]
    pub fn new(dimension: usize, reason: impl Into<String>) -> Self {
        Self {
            model_id: "disabled".to_string(),
            dimension,
            reason: reason.into(),
        }
    }

    /// The reason this implementation reports.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl Embedder for Disabled {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn status(&self) -> EmbedderStatus {
        EmbedderStatus::Disabled {
            reason: self.reason.clone(),
        }
    }

    fn embed(&self, _text: &str) -> Result<Vec<f32>, MlError> {
        Err(MlError::Unavailable {
            reason: self.reason.clone(),
        })
    }
}
