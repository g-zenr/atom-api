//! Local machine learning: search by meaning, note matching and the embedding
//! store.
//!
//! The feature is [`ADR-0008`](../../docs/decisions/ADR-0008-local-pure-rust-ml.md):
//! inference runs in this process, in pure Rust, over a local model file, and
//! **no request leaves the machine**. The public contract is
//! [`docs/features/ml/api.md`](../../docs/features/ml/api.md); the build contract
//! is [`docs/features/ml/architecture.md`](../../docs/features/ml/architecture.md) §12.
//!
//! # What exists, and what does not
//!
//! The boundary ([`Embedder`]), the deterministic embedder, the disabled
//! embedder, the storage, the three endpoints and the whole degradation rule are
//! implemented. The **Burn encoder is not**: the spike network in
//! [`burn_embedder`] runs a lookup, a mean-pool and the wrong projection, and its
//! vectors are not semantically meaningful, so
//! [`embedder::select`] reports `Disabled` for `ml.backend = flex` even when a
//! model file is present. That is the pre-agreed fallback of
//! [`ADR-0008`](../../docs/decisions/ADR-0008-local-pure-rust-ml.md) §11, and it
//! is recorded in [`docs/features/ml/status.md`](../../docs/features/ml/status.md).
//!
//! # The `ml` Cargo feature
//!
//! The feature gates `burn` and [`burn_embedder`], so
//! `cargo check --no-default-features` builds the API without the model stack.
//! The trait, the two other implementations, the service and the three routes
//! are always compiled: the router, the route table and the OpenAPI document
//! describe one surface in both builds rather than two that can drift.

pub mod dto;
pub mod embedder;
pub mod handlers;
pub mod repository;
pub mod service;

pub mod deterministic;
pub mod disabled;
pub mod hash_vector;

#[cfg(feature = "ml")]
pub mod burn_embedder;

#[cfg(feature = "ml")]
pub use burn_embedder::{BurnEmbedder, WeightSource};

pub use deterministic::Deterministic;
pub use disabled::Disabled;
pub use embedder::{
    CAUSE_ENCODER_UNAVAILABLE, DEFAULT_DIMENSION, Embedder, EmbedderStatus, MlError,
    REASON_ML_DISABLED, REASON_MODEL_MISSING, UNIT_NORM_TOLERANCE, l2_norm, normalise, select,
};
pub use repository::{MlRepository, SourceType, StoredVector};
pub use service::MlService;
