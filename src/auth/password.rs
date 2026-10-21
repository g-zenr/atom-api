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

/// A valid Argon2id hash, used only if the dummy hash cannot be computed.
///
/// The parameters are deliberately cheap: the fallback must not be more
/// expensive than a real verification.
pub const DUMMY_HASH: &str = "$argon2id$v=19$m=8192,t=1,p=1$/GHfNTpCo9BIiigqJbCL7g$g1o54Bv1IGr1rIM64BskDw0on/Gst4RO+FyLW5meQ1c";

/// Hashes and verifies passwords with Argon2id.
#[derive(Debug, Clone)]
pub struct PasswordHasher {
    params: Argon2Params,
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
    /// account-enumeration timing channel open.
    pub fn new(params: Argon2Params) -> Self {
        let dummy_hash = hash_password_with(&params, "atom-timing-equalisation")
            .unwrap_or_else(|_| DUMMY_HASH.to_string());

        Self { params, dummy_hash }
    }

    /// The hash used to equalise the time taken for an unknown email address.
    pub fn dummy_hash(&self) -> &str {
        &self.dummy_hash
    }

    /// Builds the Argon2id instance for the configured cost parameters.
    fn argon2(&self) -> Result<Argon2<'static>, AppError> {
        let params = Params::new(
            self.params.memory_kib,
            self.params.iterations,
            self.params.parallelism,
            None,
        )
        .map_err(|error| AppError::Internal(format!("invalid argon2 parameters: {error}")))?;

        Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
    }

    /// Hashes a password, producing a PHC-formatted Argon2id string.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Internal`] when the configured parameters are
    /// rejected by the Argon2 implementation or when hashing fails.
    pub fn hash(&self, password: &str) -> AppResult<String> {
        hash_password_with(&self.params, password)
    }

    /// Verifies a password against a stored hash.
    ///
    /// Returns `false` for a malformed stored hash instead of an error: a
    /// corrupt hash must fail the authentication, not the request pipeline.
    pub fn verify(&self, password: &str, stored_hash: &str) -> bool {
        let Ok(parsed) = PasswordHash::new(stored_hash) else {
            return false;
        };

        let Ok(hasher) = self.argon2() else {
            return false;
        };

        hasher.verify_password(password.as_bytes(), &parsed).is_ok()
    }
}

/// Hashes a password with the given cost parameters.
fn hash_password_with(params: &Argon2Params, password: &str) -> AppResult<String> {
    let argon2_params = Params::new(
        params.memory_kib,
        params.iterations,
        params.parallelism,
        None,
    )
    .map_err(|error| AppError::Internal(format!("invalid argon2 parameters: {error}")))?;

    let hasher = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon2_params);

    let mut salt_bytes = [0u8; SALT_BYTES];
    rand::rng().fill_bytes(&mut salt_bytes);
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|error| AppError::Internal(format!("could not encode salt: {error}")))?;

    use argon2::PasswordHasher as _;

    hasher
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
}
