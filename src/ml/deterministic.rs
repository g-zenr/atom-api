//! The hash-based embedder: the test double that needs no model file.
//!
//! ADR-0008 §2 puts this implementation behind **every automated test**, so the
//! plumbing — storage, upsert, re-rank, degradation — can be covered without a
//! model artifact and without a network. It is deliberately *not* semantically
//! meaningful: it proves that vectors flow, not that they are good.
//!
//! Construction reads no file and contact no network, which is what the spike
//! asserts.

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
        // Counter mode over SHA-256 fills the vector, and `normalise` rescales
        // it and refuses one that has no unit form. Both steps live in
        // `super::hash_vector`, which `atom-seed` also compiles in, so the
        // seeder's rows and a query vector at run time come from the same code.
        let mut vector = super::hash_vector::raw(text, self.dimension);

        super::embedder::normalise(&mut vector, self.dimension)?;

        Ok(vector)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ml::embedder::{UNIT_NORM_TOLERANCE, l2_norm};

    #[test]
    fn the_deterministic_embedder_is_stable() {
        let first = Deterministic::new(64);
        let second = Deterministic::new(64);

        let a = first
            .embed("kubernetes rollout")
            .expect("the embedder works");
        let b = first
            .embed("kubernetes rollout")
            .expect("the embedder works");
        let c = second
            .embed("kubernetes rollout")
            .expect("the embedder works");

        assert_eq!(a, b, "the same input must produce the same vector");
        assert_eq!(a, c, "two instances must agree");

        let other = first.embed("a different note").expect("the embedder works");
        assert_ne!(
            a, other,
            "a different input must produce a different vector"
        );
    }

    #[test]
    fn the_deterministic_embedder_has_the_configured_dimension() {
        for dimension in [1, 3, 64, DEFAULT_DIMENSION, 1_000] {
            let embedder = Deterministic::new(dimension);
            let vector = embedder.embed("hello").expect("the embedder works");

            assert_eq!(vector.len(), embedder.dimension());
            assert_eq!(vector.len(), dimension);
        }

        // The identifier records the dimension, so a model change is a new id
        // rather than a rewrite of the old rows (ADR-0008 §9).
        assert_eq!(
            Deterministic::new(384).model_id(),
            "deterministic-sha256-v1-384"
        );
        assert_eq!(
            Deterministic::new(8).model_id(),
            "deterministic-sha256-v1-8"
        );
    }

    #[test]
    fn the_deterministic_embedder_returns_a_unit_vector() {
        let embedder = Deterministic::with_default_dimension();

        for text in ["", " ", "kubernetes", "a sentence with several words in it"] {
            let vector = embedder.embed(text).expect("the embedder works");

            // `vector` is `REAL[]`: the norm is compared with a tolerance,
            // never with equality.
            assert!(
                (1.0 - l2_norm(&vector)).abs() <= UNIT_NORM_TOLERANCE,
                "the vector for {text:?} must be unit length"
            );
        }
    }

    #[test]
    fn the_deterministic_embedder_performs_no_io() {
        // The testable half of ML-R01: construction takes no path, no file and
        // no client, and the only argument is a dimension. If this ever needed
        // a model artifact the signature would have to change, which is what
        // makes the assertion meaningful.
        let embedder = Deterministic::with_default_dimension();

        assert!(embedder.status().is_ready());
        assert_eq!(embedder.model_id(), "deterministic-sha256-v1-384");
    }

    #[test]
    fn the_model_identifier_is_the_documented_one() {
        // ADR-0008 §9 retired `burn-minilm-l6-v2-spike-v1` because it named an
        // unnormalised, meaningless vector. The test double has its own name.
        assert!(!Deterministic::new(384).model_id().contains("burn"));
    }
}
