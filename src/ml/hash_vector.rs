//! The hash-based vector, as two dependency-free functions.
//!
//! [`super::deterministic::Deterministic`] is the test double of
//! `ADR-0008` §2, and `src/bin/atom-seed.rs` must build **the same** vector
//! without linking this library crate — the seeder is a development tool and
//! deliberately does not pull the library's build graph (Burn, the embedded
//! Swagger UI bundle) into its own build. The seeder therefore includes this
//! file **by path**, so there is one implementation rather than two that can
//! drift:
//!
//! ```text
//! #[path = "../ml/hash_vector.rs"]
//! mod hash_vector;
//! ```
//!
//! Two consequences, and both are load-bearing:
//!
//! * **This file may not use `crate::` or `super::`.** It is compiled into two
//!   different crates, so a reference to either would not resolve in one of
//!   them.
//! * **Changing the algorithm changes what every stored deterministic vector
//!   means.** `model_id` records the algorithm and the dimension
//!   (`deterministic-sha256-v1-{dim}`), so a change is a new identifier and a
//!   reindex, never a silent rewrite (`ADR-0008` §9).
//!
//! The vector is deliberately *not* semantically meaningful: it proves that
//! vectors flow, not that they are good
//! ([`docs/features/ml/status.md`](../../docs/features/ml/status.md) §2.1).

/// The Euclidean norm of a vector, as `f64` so a long vector does not lose
/// precision while summing.
#[must_use]
pub fn l2_norm(values: &[f32]) -> f64 {
    values
        .iter()
        .map(|value| f64::from(*value) * f64::from(*value))
        .sum::<f64>()
        .sqrt()
}

/// Rescales `values` to unit length in place.
///
/// Returns `false` when the vector has no unit form — a zero norm, or one that
/// is not finite — so the caller can report the failure in its own error type.
/// A vector that cannot be normalised must never be stored: it has no cosine
/// with anything (`ADR-0008` §9).
#[must_use]
pub fn normalise(values: &mut [f32]) -> bool {
    let norm = l2_norm(values);

    if !norm.is_finite() || norm <= f64::EPSILON {
        return false;
    }

    let scale = 1.0 / norm;
    for value in values.iter_mut() {
        // Every value is scaled by the same factor. A finite `f64` in this
        // range cannot overflow an `f32`, and a non-finite input was refused by
        // the norm check above.
        *value = (*value as f64 * scale) as f32;
    }

    true
}

/// Builds the raw vector: `dimension` values in `[-1, 1)`, not yet normalised.
///
/// SHA-256 in counter mode: the text is hashed with a block counter until the
/// requested length is filled, four bytes at a time. The result is stable
/// forever for one input and one dimension, and it needs no file, no network
/// and no RNG.
///
/// The caller normalises the result with [`normalise`]; keeping the two steps
/// apart is what lets `Embedder::embed` enforce the unit-length invariant on
/// every implementation rather than trusting each builder.
#[must_use]
pub fn raw(text: &str, dimension: usize) -> Vec<f32> {
    use sha2::{Digest, Sha256};

    let mut vector = Vec::with_capacity(dimension);
    let mut counter: u32 = 0;

    while vector.len() < dimension {
        let mut hasher = Sha256::new();
        hasher.update(counter.to_le_bytes());
        hasher.update(text.as_bytes());
        let digest = hasher.finalize();

        for chunk in digest.as_slice().as_chunks::<4>().0 {
            if vector.len() == dimension {
                break;
            }

            let raw = u32::from_le_bytes(*chunk);
            // Map the unsigned value onto [-1, 1).
            vector.push((raw as f32 / u32::MAX as f32) * 2.0 - 1.0);
        }

        counter = counter.wrapping_add(1);
    }

    vector
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_raw_vector_has_the_requested_length() {
        for dimension in [0, 1, 3, 4, 64, 384] {
            assert_eq!(raw("hello", dimension).len(), dimension);
        }
    }

    #[test]
    fn the_raw_vector_is_stable_and_input_dependent() {
        assert_eq!(raw("kubernetes", 32), raw("kubernetes", 32));
        assert_ne!(raw("kubernetes", 32), raw("terraform", 32));
    }

    /// Every component stays inside the documented `[-1, 1)` range, which is
    /// what makes the normalisation well defined and the model identifier's
    /// promise (`deterministic-sha256-v1-{dim}`) meaningful.
    #[test]
    fn the_algorithm_stays_inside_the_documented_range() {
        let values = raw("atom", 4);

        assert_eq!(values.len(), 4);
        assert!(values.iter().all(|value| (-1.0..1.0).contains(value)));
    }

    #[test]
    fn normalising_produces_a_unit_vector() {
        let mut values = raw("kubernetes rollout", 384);

        assert!(normalise(&mut values));
        assert!((1.0 - l2_norm(&values)).abs() <= 1e-5);
    }

    #[test]
    fn a_zero_vector_cannot_be_normalised() {
        let mut values = vec![0.0_f32; 8];

        assert!(!normalise(&mut values));
    }

    #[test]
    fn a_non_finite_vector_cannot_be_normalised() {
        let mut values = vec![f32::INFINITY, 1.0, 0.0, 0.0];

        assert!(!normalise(&mut values));
    }
}
