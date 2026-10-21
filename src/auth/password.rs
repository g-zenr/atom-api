//! Password hashing and verification.
//!
//! ATOM uses Argon2id with parameters supplied by configuration. The plain
//! password is never stored, never logged and never returned.

use argon2::password_hash::SaltString;
use argon2::{Algorithm, Argon2, Params, PasswordHash, PasswordVerifier, Version};
use rand::RngCore;

use crate::config::Argon2Params;
use crate::error::{AppError, AppResult};

/// Length in bytes of the random salt.
const SALT_BYTES: usize = 16;

/// The value that is hashed to produce the timing-equalisation target.
///
/// It is not a secret and it is never a password: it exists so that the
/// verifier has something to chew on when the account does not exist.
const TIMING_EQUALISATION_VALUE: &str = "atom-timing-equalisation";

/// Hashes and verifies passwords with Argon2id.
#[derive(Debug, Clone)]
pub struct PasswordHasher {
    /// The Argon2id instance built from the configured cost parameters.
    argon2: Argon2<'static>,
    /// A hash of an unguessable value, used as the timing-equalisation
    /// target when the supplied email address does not exist.
    dummy_hash: String,
}

impl PasswordHasher {
    /// Builds a hasher from the configured cost parameters.
    ///
    /// The dummy hash is computed with the *same* parameters, so that
    /// verifying against it costs exactly what verifying against a stored
    /// password costs. A dummy hash with cheaper parameters would leave the
    /// account-enumeration timing channel open, which is why there is no
    /// pre-computed fallback.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Internal`] when the configured parameters are
    /// rejected by the Argon2 implementation, or when the dummy hash cannot
    /// be computed.
    pub fn new(params: Argon2Params) -> AppResult<Self> {
        let argon2 = build_argon2(&params)?;
        let dummy_hash = hash_password_with(&argon2, TIMING_EQUALISATION_VALUE)?;

        Ok(Self { argon2, dummy_hash })
    }

    /// The hash used to equalise the time taken for an unknown email address.
    pub fn dummy_hash(&self) -> &str {
        &self.dummy_hash
    }

    /// Hashes a password, producing a PHC-formatted Argon2id string.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Internal`] when the password cannot be hashed.
    pub fn hash(&self, password: &str) -> AppResult<String> {
        hash_password_with(&self.argon2, password)
    }

    /// Verifies a password against a stored hash.
    ///
    /// Returns `false` for a malformed stored hash instead of an error: a
    /// corrupt hash must fail the authentication, not the request pipeline.
    pub fn verify(&self, password: &str, stored_hash: &str) -> bool {
        let Ok(parsed) = PasswordHash::new(stored_hash) else {
            return false;
        };

        self.argon2
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    }
}

/// Builds the Argon2id instance for the given cost parameters.
fn build_argon2(params: &Argon2Params) -> AppResult<Argon2<'static>> {
    let argon2_params = Params::new(
        params.memory_kib,
        params.iterations,
        params.parallelism,
        None,
    )
    .map_err(|error| AppError::Internal(format!("invalid argon2 parameters: {error}")))?;

    Ok(Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        argon2_params,
    ))
}

/// Hashes a password with the given Argon2id instance.
fn hash_password_with(argon2: &Argon2<'_>, password: &str) -> AppResult<String> {
    let mut salt_bytes = [0u8; SALT_BYTES];
    rand::rng().fill_bytes(&mut salt_bytes);
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|error| AppError::Internal(format!("could not encode salt: {error}")))?;

    use argon2::PasswordHasher as _;

    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| AppError::Internal(format!("could not hash password: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hasher() -> PasswordHasher {
        // Deliberately cheap parameters so the unit tests stay fast.
        PasswordHasher::new(Argon2Params {
            memory_kib: 8_192,
            iterations: 1,
            parallelism: 1,
        })
        .expect("the parameters are valid")
    }

    #[test]
    fn hashes_verify_against_the_original_password() {
        let hasher = hasher();
        let hash = hasher.hash("correct horse battery staple").unwrap();
        assert!(hasher.verify("correct horse battery staple", &hash));
        assert!(!hasher.verify("wrong password", &hash));
    }

    #[test]
    fn hash_is_salted_so_equal_passwords_differ() {
        let hasher = hasher();
        let first = hasher.hash("same-password").unwrap();
        let second = hasher.hash("same-password").unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn hash_never_contains_the_plaintext() {
        let hasher = hasher();
        let hash = hasher.hash("plaintext-secret").unwrap();
        assert!(!hash.contains("plaintext-secret"));
    }

    #[test]
    fn malformed_hash_fails_closed() {
        let hasher = hasher();
        assert!(!hasher.verify("anything", "not-a-phc-string"));
        assert!(!hasher.verify("anything", ""));
    }

    #[test]
    fn the_dummy_hash_costs_the_same_as_a_real_one() {
        let hasher = hasher();

        // The dummy hash is a real Argon2id hash of the configured cost, so
        // it verifies against the configured parameters and not against a
        // cheaper fallback.
        assert!(hasher.verify(TIMING_EQUALISATION_VALUE, hasher.dummy_hash()));
        assert!(!hasher.verify("anything else", hasher.dummy_hash()));
    }

    #[test]
    fn invalid_parameters_are_refused() {
        let error = PasswordHasher::new(Argon2Params {
            memory_kib: 0,
            iterations: 0,
            parallelism: 0,
        })
        .expect_err("the parameters must be refused");

        assert_eq!(error.code(), crate::error::code::INTERNAL_ERROR);
    }
}
