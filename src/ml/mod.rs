//! Local machine learning — **spike surface only**.
//!
//! This module exists to answer the D4.M1 question: can this machine run a
//! sentence-embedding forward pass in-process, in pure Rust, with Burn on the
//! `flex` backend, fast enough and small enough for an endpoint to depend on?
//!
//! It contains the `Embedder` boundary from ADR-0008 §2 and just enough of the
//! three named implementations to exercise the trait. It deliberately contains
//! **no** service, repository, DTO, route or configuration: those are chains
//! M3–M6, and nothing here is reachable from the HTTP API.
//!
//! The whole module is gated behind the `ml` Cargo feature, so
//! `cargo check --no-default-features` builds the API without Burn.

pub mod burn_embedder;
pub mod deterministic;
pub mod disabled;
pub mod embedder;

pub use burn_embedder::{BurnEmbedder, WeightSource};
pub use deterministic::Deterministic;
pub use disabled::Disabled;
pub use embedder::{DEFAULT_DIMENSION, Embedder, EmbedderStatus, MlError, l2_norm};
