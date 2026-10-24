//! The `Embedder` boundary, and the selection made once at start-up.
//!
//! Mirrors [`ADR-0008`](../../docs/decisions/ADR-0008-local-pure-rust-ml.md) §2
//! and §9, and [`docs/features/ml/architecture.md`](../../docs/features/ml/architecture.md)
//! §12.1–§12.2.
//!
//! Two properties are enforced here rather than left to each caller:
//!
//! * **`embed` returns a unit vector.** Every implementation funnels its output
//!   through [`normalise`], so the value stored in `embeddings.vector` and the
//!   query vector are both unit length and cosine similarity is exactly the dot
//!   product. A zero or non-finite norm has no unit vector, so it is an error
//!   and no row is written.
//! * **The choice is made once.** [`select`] runs in `AppState::new`; a request
//!   never re-reads a file and never retries a failed load.

use std::error::Error;
use std::fmt;
use std::sync::Arc;

use crate::config::settings::{MlBackend, MlSettings};

use super::deterministic::Deterministic;
use super::disabled::Disabled;

/// The vector length `ml.dimension` defaults to, and the one MiniLM produces.
pub const DEFAULT_DIMENSION: usize = 384;

/// The published degradation reason for a deployment with ML switched off.
///
/// It is also the `MlError::Unavailable` code, so the two vocabularies cannot
/// drift: [`MlError::code`] and [`super::dto::DegradedReason::as_str`] agree on
/// every value.
pub const REASON_ML_DISABLED: &str = "ml_disabled";

/// The published degradation reason for every unusable-model state, including a
/// missing weight file, a missing vocabulary and a degenerate vector.
///
/// [`ADR-0008`](../../docs/decisions/ADR-0008-local-pure-rust-ml.md) §9 fixes
/// that: "the model produced something unusable is `model_missing` to a
/// client". A fifth client-visible code would be a contract change.
pub const REASON_MODEL_MISSING: &str = "model_missing";

/// The internal cause logged once when the Burn backend has no complete
/// encoder.
///
/// It is **not** a client-visible value: `degraded_reason` is a three-value
/// enum and adding a fourth is a contract change
/// ([`docs/features/ml/architecture.md`](../../docs/features/ml/architecture.md)
/// §12.9). It names the reason the `flex` backend is `Disabled` in this build,
/// which is that the six-layer encoder of §12.3 has not been written.
pub const CAUSE_ENCODER_UNAVAILABLE: &str = "ml_model_unavailable";

/// The tolerance the unit-length invariant is stated with.
///
/// `(1.0 - l2_norm(&v)).abs() <= 1e-5` for every vector `embed` returns. It is
/// asserted by the tests and documented in §2 of the feature architecture.
pub const UNIT_NORM_TOLERANCE: f64 = 1e-5;

/// Why an embedder could not produce a vector.
///
/// A variant carries a fixed reason code plus a human detail. Neither is
/// returned to a client: the service maps them onto `degraded_reason` and the
/// detail is logged once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MlError {
    /// ML is switched off, or the model file is absent. A configuration state,
    /// never a server error.
    Unavailable { reason: String },
    /// The model file is present but could not be read, parsed or used.
    Model { message: String },
}

impl MlError {
    /// The fixed code a degraded response may report.
    ///
    /// An `Unavailable` error carries the reason the embedder reported, which
    /// is already one of the published values: a disabled embedder that is
    /// disabled because its model is missing must degrade as `model_missing`,
    /// not as `ml_disabled`. Anything else maps to `ml_disabled`, because an
    /// unavailable embedder with no recognisable reason is "switched off".
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Model { .. } => REASON_MODEL_MISSING,
            Self::Unavailable { reason } if reason == REASON_MODEL_MISSING => REASON_MODEL_MISSING,
            Self::Unavailable { .. } => REASON_ML_DISABLED,
        }
    }
}

impl fmt::Display for MlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable { reason } => write!(f, "embedder unavailable: {reason}"),
            Self::Model { message } => write!(f, "embedder model error: {message}"),
        }
    }
}

impl Error for MlError {}

/// Whether an embedder can serve a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedderStatus {
    /// The embedder can produce vectors.
    Ready,
    /// The embedder reports unavailability; the service degrades rather than
    /// failing the request.
    Disabled { reason: String },
}

impl EmbedderStatus {
    /// `true` when the embedder can serve a call.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// A local text-to-vector function.
///
/// `embed` is synchronous and CPU-bound: a caller on the async runtime must
/// move it to `spawn_blocking` (see `docs/features/ml/architecture.md` §2.1).
/// It returns an error and never panics, and it performs no I/O after
/// construction.
pub trait Embedder: Send + Sync {
    /// A stable identifier stored with every row, for example
    /// `burn-all-minilm-l6-v2-v2`.
    fn model_id(&self) -> &str;

    /// The length of every vector this embedder produces.
    fn dimension(&self) -> usize;

    /// Whether the embedder can serve a call. Decided at construction and
    /// never changed: a request never retries a failed load.
    fn status(&self) -> EmbedderStatus;

    /// Embeds one text. The returned vector has `dimension()` elements and is
    /// unit length, or the call returns an error.
    fn embed(&self, text: &str) -> Result<Vec<f32>, MlError>;
}

/// The Euclidean norm of a vector, as `f64` so a long vector does not lose
/// precision while summing.
///
/// The implementation lives in [`super::hash_vector`], which is compiled into
/// `atom-seed` as well; see that module for why.
#[must_use]
pub fn l2_norm(values: &[f32]) -> f64 {
    super::hash_vector::l2_norm(values)
}

/// Rescales `values` to unit length in place, or reports that it cannot.
///
/// This is the one place the write-time invariant of
/// [`ADR-0008`](../../docs/decisions/ADR-0008-local-pure-rust-ml.md) §9 lives:
/// every implementation calls it as the last step of `embed`, so no code path
/// can return an unnormalised vector and `embeddings.vector` cannot hold two
/// conventions.
///
/// # Errors
///
/// Returns [`MlError::Model`] when `values` does not have `dimension` elements,
/// and when its norm is zero or not finite: neither has a unit vector, so
/// returning one would be a lie and storing one would corrupt the table.
pub fn normalise(values: &mut [f32], dimension: usize) -> Result<(), MlError> {
    if values.len() != dimension {
        return Err(MlError::Model {
            message: format!(
                "the model produced {} values, expected {dimension}",
                values.len()
            ),
        });
    }

    if !super::hash_vector::normalise(values) {
        return Err(MlError::Model {
            message: format!(
                "the model produced a vector with norm {}, which has no unit form",
                super::hash_vector::l2_norm(values)
            ),
        });
    }

    Ok(())
}

/// Selects the embedder once, at start-up.
///
/// The decision table is
/// [`docs/features/ml/architecture.md`](../../docs/features/ml/architecture.md)
/// §12.2:
///
/// ```text
/// ml.enabled = false                    -> Disabled("ml_disabled")
/// ml.enabled = true, backend=deterministic -> Deterministic(dim)
/// ml.enabled = true, backend=flex, artifacts missing -> Disabled("model_missing")
/// ml.enabled = true, backend=flex, artifacts present -> see below
/// ```
///
/// **The `flex` branch never returns a working embedder in this build.** The
/// six-layer encoder and the WordPiece tokenizer of §12.3 are not implemented:
/// the spike network in [`super::burn_embedder`] runs a lookup, a mean-pool and
/// the wrong projection, and its vectors carry no meaning. Reporting it as
/// `Ready` would let the service store them as if they did, so the branch
/// reports `Disabled` with the published `model_missing` code and logs the real
/// cause, [`CAUSE_ENCODER_UNAVAILABLE`], once. That is the pre-agreed fallback
/// of [`ADR-0008`](../../docs/decisions/ADR-0008-local-pure-rust-ml.md) §11: the
/// endpoints exist and degrade correctly, and no requirement is quietly
/// redefined.
///
/// A missing or unreadable artifact is a **state**, never a start-up failure
/// ([`ADR-0008`](../../docs/decisions/ADR-0008-local-pure-rust-ml.md) §8.3), so
/// this function cannot fail: it always returns an embedder.
#[must_use]
pub fn select(settings: &MlSettings) -> Arc<dyn Embedder> {
    let dimension = settings.dimension;

    if !settings.enabled {
        // `ml.enabled = false` wins over `ml.backend`, whatever it says.
        return Arc::new(Disabled::new(dimension, REASON_ML_DISABLED));
    }

    match settings.backend {
        MlBackend::Deterministic => Arc::new(Deterministic::new(dimension)),
        MlBackend::Flex => select_burn(settings, dimension),
    }
}

/// The `flex` branch of [`select`].
///
/// The two artifact checks run first, in the order an operator would make them,
/// because the cause is logged: "the weights are missing" and "the encoder is
/// not built into this binary" are different problems with the same client
/// answer.
fn select_burn(settings: &MlSettings, dimension: usize) -> Arc<dyn Embedder> {
    if let Some(cause) = artifact_fault(settings) {
        tracing::warn!(
            cause,
            "ml.enabled is true and ml.backend is flex, but a model artifact cannot be used; \
             the embedder is Disabled and every scored request degrades. Start-up continues \
             (ADR-0008 §8.3)."
        );

        return Arc::new(Disabled::new(dimension, REASON_MODEL_MISSING));
    }

    tracing::warn!(
        cause = CAUSE_ENCODER_UNAVAILABLE,
        "ml.enabled is true and both model artifacts are present, but the Burn encoder these \
         settings describe is not implemented in this build (ADR-0008 §11); the embedder is \
         Disabled and every scored request degrades to lexical results. Reindex and match \
         answer 409."
    );

    Arc::new(Disabled::new(dimension, REASON_MODEL_MISSING))
}

/// Describes the first artifact that cannot be used, if any.
///
/// The two `is_file` calls are the only file I/O in the module, and they run
/// once, at start-up, before the socket is bound. No request path reads a file.
fn artifact_fault(settings: &MlSettings) -> Option<&'static str> {
    let Some(model) = settings.model_file() else {
        return Some("ml.model_path is not set");
    };

    if !model.is_file() {
        return Some("ml.model_path does not name a readable file");
    }

    let Some(tokenizer) = settings.tokenizer_file() else {
        return Some("ml.tokenizer_path is not set and there is no sibling vocab.txt");
    };

    if !tokenizer.is_file() {
        return Some("the WordPiece vocabulary does not name a readable file");
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ml::dto::DegradedReason;

    /// A settings value with `ml.enabled` and a backend.
    fn settings(enabled: bool, backend: MlBackend) -> MlSettings {
        MlSettings {
            enabled,
            backend,
            ..MlSettings::default()
        }
    }

    /// The path of a file that exists, in the system temporary directory.
    fn existing_file(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("atom-ml-test-{name}"));
        std::fs::write(&path, b"placeholder").expect("the temporary file must be writable");
        path
    }

    #[test]
    fn ml_disabled_selects_the_disabled_embedder() {
        // `ml.enabled = false` wins whatever the backend says, so a deployment
        // that never turns ML on never looks at a model file.
        for backend in [MlBackend::Flex, MlBackend::Deterministic] {
            let embedder = select(&settings(false, backend));

            assert_eq!(
                embedder.status(),
                EmbedderStatus::Disabled {
                    reason: REASON_ML_DISABLED.to_string()
                }
            );
            assert!(!embedder.status().is_ready());
        }
    }

    #[test]
    fn a_missing_model_selects_the_disabled_embedder() {
        let mut configuration = settings(true, MlBackend::Flex);
        configuration.model_path = "/a/path/that/does/not/exist.safetensors".to_string();

        let embedder = select(&configuration);

        // `model_missing` is the published reason; the internal cause is logged
        // once, at start-up, and never reaches a client.
        assert_eq!(
            embedder.status(),
            EmbedderStatus::Disabled {
                reason: REASON_MODEL_MISSING.to_string()
            }
        );
        assert_eq!(
            embedder
                .embed("anything")
                .expect_err("a disabled embedder cannot embed")
                .code(),
            REASON_MODEL_MISSING
        );
        assert_eq!(
            DegradedReason::from_reason(REASON_MODEL_MISSING),
            DegradedReason::ModelMissing
        );
    }

    #[test]
    fn a_missing_tokenizer_selects_the_disabled_embedder() {
        let weights = existing_file("weights-without-vocabulary.safetensors");
        let mut configuration = settings(true, MlBackend::Flex);
        configuration.model_path = weights.display().to_string();
        // No `vocab.txt` beside it and no `ml.tokenizer_path`.
        configuration.tokenizer_path = String::new();

        let embedder = select(&configuration);

        assert!(matches!(
            embedder.status(),
            EmbedderStatus::Disabled { reason } if reason == REASON_MODEL_MISSING
        ));

        let _ = std::fs::remove_file(weights);
    }

    #[test]
    fn both_artifacts_present_still_report_the_disabled_embedder() {
        // The honest result of this change: the artifact gate passes and the
        // encoder still does not exist, so the branch stays `Disabled` instead
        // of returning vectors that mean nothing (ADR-0008 §11).
        let weights = existing_file("complete-weights.safetensors");
        let vocabulary = existing_file("complete-vocab.txt");

        let mut configuration = settings(true, MlBackend::Flex);
        configuration.model_path = weights.display().to_string();
        configuration.tokenizer_path = vocabulary.display().to_string();

        let embedder = select(&configuration);

        assert_eq!(
            embedder.status(),
            EmbedderStatus::Disabled {
                reason: REASON_MODEL_MISSING.to_string()
            }
        );
        assert!(
            embedder.embed("kubernetes rollout").is_err(),
            "a disabled embedder must never produce a vector"
        );

        let _ = std::fs::remove_file(weights);
        let _ = std::fs::remove_file(vocabulary);
    }

    #[test]
    fn the_deterministic_backend_is_ready_and_needs_no_file() {
        let mut configuration = settings(true, MlBackend::Deterministic);
        // A path that does not exist must not matter: this backend reads nothing.
        configuration.model_path = "/a/path/that/does/not/exist.safetensors".to_string();

        let embedder = select(&configuration);

        assert!(embedder.status().is_ready());
        assert_eq!(embedder.dimension(), DEFAULT_DIMENSION);
        assert_eq!(embedder.model_id(), "deterministic-sha256-v1-384");
    }

    #[test]
    fn the_disabled_embedder_reports_its_reason() {
        let embedder = Disabled::new(DEFAULT_DIMENSION, REASON_ML_DISABLED);

        assert_eq!(
            embedder.status(),
            EmbedderStatus::Disabled {
                reason: REASON_ML_DISABLED.to_string()
            }
        );
        // The dimension is still reported, so the response shape does not
        // change when the embedder is disabled.
        assert_eq!(embedder.dimension(), DEFAULT_DIMENSION);

        let error = embedder
            .embed("anything")
            .expect_err("a disabled embedder cannot embed");
        assert!(matches!(error, MlError::Unavailable { .. }));
        assert_eq!(error.code(), REASON_ML_DISABLED);
    }

    #[test]
    fn an_unknown_backend_is_refused_at_start_up() {
        // The environment layer refuses it with the variable named ...
        assert_eq!(MlBackend::parse("cuda").unwrap_err(), {
            crate::config::ConfigError::Invalid {
                key: "APP_ML_BACKEND".to_string(),
                value: "cuda".to_string(),
                expected: "one of: flex, deterministic".to_string(),
            }
        });

        // ... and the file layer refuses it as a parse error, because the type
        // has exactly two variants. Neither path has a silent fallback.
        assert_eq!(MlBackend::parse(" flex ").unwrap(), MlBackend::Flex);
        assert_eq!(
            MlBackend::parse("DETERMINISTIC").unwrap(),
            MlBackend::Deterministic
        );
    }

    #[test]
    fn normalising_a_zero_vector_is_an_error() {
        let mut values = vec![0.0_f32; 8];

        let error = normalise(&mut values, 8).expect_err("a zero vector has no unit form");

        assert!(matches!(error, MlError::Model { .. }));
        assert_eq!(error.code(), REASON_MODEL_MISSING);
    }

    #[test]
    fn normalising_a_vector_of_the_wrong_length_is_an_error() {
        let mut values = vec![0.5_f32; 4];

        assert!(normalise(&mut values, 8).is_err());
    }

    #[test]
    fn a_non_finite_value_is_an_error() {
        let mut values = vec![f32::INFINITY, 1.0, 0.0, 0.0];

        assert!(normalise(&mut values, 4).is_err());
    }

    #[test]
    fn normalising_produces_a_unit_vector() {
        let mut values = vec![3.0_f32, 4.0_f32];

        normalise(&mut values, 2).expect("a non-zero vector has a unit form");

        // The exact norm is asserted with a tolerance: `vector` is `REAL[]`,
        // so the values are single precision and exact equality would be a
        // test of the rounding, not of the invariant.
        assert!((1.0 - l2_norm(&values)).abs() <= UNIT_NORM_TOLERANCE);
        assert!((values[0] - 0.6).abs() < 1e-6);
        assert!((values[1] - 0.8).abs() < 1e-6);
    }
}
