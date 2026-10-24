//! The `Embedder` boundary.
//!
//! Mirrors [`ADR-0008`](../../docs/decisions/ADR-0008-local-pure-rust-ml.md) §2.
//! This is the spike's sketch of the contract, not the shipped module: the
//! service, the storage and the degradation rules are later chains.

use std::error::Error;
use std::fmt;

/// The vector length `ml.dimension` defaults to, and the one MiniLM produces.
pub const DEFAULT_DIMENSION: usize = 384;

/// Why an embedder could not produce a vector.
///
/// A variant carries a fixed reason code plus a human detail. Neither is
/// returned to a client: the service maps them onto `degraded_reason`
/// (ADR-0008 §5) and the detail is logged once.
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
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unavailable { .. } => "ml_disabled",
            Self::Model { .. } => "model_missing",
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

/// Whether an embedder can serve a call. ADR-0008 §2.
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
    /// `burn-minilm-l6-v2-v1`.
    fn model_id(&self) -> &str;

    /// The length of every vector this embedder produces.
    fn dimension(&self) -> usize;

    /// Whether the embedder can serve a call.
    fn status(&self) -> EmbedderStatus;

    /// Embed one text. The returned vector has `dimension()` elements.
    fn embed(&self, text: &str) -> Result<Vec<f32>, MlError>;
}

/// The Euclidean norm of a vector, as `f64` so a long vector does not lose
/// precision while summing.
#[must_use]
pub fn l2_norm(values: &[f32]) -> f64 {
    values
        .iter()
        .map(|v| f64::from(*v) * f64::from(*v))
        .sum::<f64>()
        .sqrt()
}
