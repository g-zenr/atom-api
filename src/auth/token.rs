//! Access-token issuing and verification, and opaque refresh-token generation.
//!
//! Access tokens are short-lived HS256 JWTs. Refresh tokens are opaque random
//! strings; only their SHA-256 hash is persisted, so a database disclosure
//! does not yield usable refresh tokens.
//!
//! The design is recorded in `docs/decisions/ADR-0003-auth-token-design.md`.

use std::time::Duration;

use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

/// Number of random bytes in a refresh token before encoding.
const REFRESH_TOKEN_BYTES: usize = 32;

/// Claims carried by an ATOM access token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    /// Subject: the user id.
    ///
    /// It is typed as a [`Uuid`] so that a malformed subject is a type error
    /// at the trust boundary rather than a string that has to be parsed
    /// afterwards. The wire format is unchanged: a UUID serialises as the
    /// hyphenated string it has always been.
    pub sub: Uuid,
    /// Convenience claim carrying the user's email.
    pub email: String,
    /// Issuer.
    pub iss: String,
    /// Audience.
    pub aud: String,
    /// Expiry, seconds since the Unix epoch.
    pub exp: i64,
    /// Issued-at, seconds since the Unix epoch.
    pub iat: i64,
    /// Unique token id, used for future revocation lists.
    pub jti: String,
}

/// Why an access token could not be verified.
///
/// The type carries the reason without any wording: the client-facing text
/// comes from the message catalogue, in the authentication service. A caller
/// must not be able to tell the failure modes apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TokenError {
    /// The token is malformed, expired, signed with another key, or issued
    /// for another issuer or audience.
    #[error("the access token could not be verified")]
    Invalid,
}

/// A freshly issued access token.
#[derive(Debug, Clone)]
pub struct IssuedAccessToken {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

/// Issues and verifies access tokens.
#[derive(Clone)]
pub struct TokenService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    issuer: String,
    audience: String,
    access_ttl: Duration,
    clock_skew: Duration,
}

impl std::fmt::Debug for TokenService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenService")
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("access_ttl", &self.access_ttl)
            .field("clock_skew", &self.clock_skew)
            .finish_non_exhaustive()
    }
}

impl TokenService {
    /// Builds a token service from configuration.
    pub fn new(
        secret: &str,
        issuer: &str,
        audience: &str,
        access_ttl: Duration,
        clock_skew: Duration,
    ) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            issuer: issuer.to_string(),
            audience: audience.to_string(),
            access_ttl,
            clock_skew,
        }
    }

    /// Issues an access token for a user.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Internal`] when the token cannot be signed.
    pub fn issue_access_token(&self, user_id: Uuid, email: &str) -> AppResult<IssuedAccessToken> {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::from_std(self.access_ttl).unwrap_or_default();

        let claims = AccessTokenClaims {
            sub: user_id,
            email: email.to_string(),
            iss: self.issuer.clone(),
            aud: self.audience.clone(),
            exp: expires_at.timestamp(),
            iat: now.timestamp(),
            jti: Uuid::new_v4().to_string(),
        };

        let token = encode(&Header::new(Algorithm::HS256), &claims, &self.encoding_key)
            .map_err(|error| AppError::Internal(format!("could not sign token: {error}")))?;

        Ok(IssuedAccessToken { token, expires_at })
    }

    /// Verifies an access token and returns its claims.
    ///
    /// # Errors
    ///
    /// Returns [`TokenError::Invalid`] for every failure mode (bad signature,
    /// expiry, wrong issuer or audience). The caller must not be able to
    /// distinguish between them.
    pub fn decode_access_token(&self, token: &str) -> Result<AccessTokenClaims, TokenError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.set_audience(&[self.audience.as_str()]);
        validation.set_required_spec_claims(&["exp", "sub", "iss", "aud"]);
        // The clock-skew allowance comes from the configuration, so that a
        // deployment whose hosts drift can widen it without a code change.
        validation.leeway = self.clock_skew.as_secs();

        decode::<AccessTokenClaims>(token, &self.decoding_key, &validation)
            .map(|data| data.claims)
            .map_err(|_| TokenError::Invalid)
    }

    /// Lifetime of the access tokens this service issues.
    pub fn access_ttl(&self) -> Duration {
        self.access_ttl
    }
}

/// A freshly generated opaque refresh token.
#[derive(Debug, Clone)]
pub struct IssuedRefreshToken {
    /// The token handed to the client. Only its hash is stored.
    pub token: String,
    /// SHA-256 hash of the token, hex encoded. This is what the database stores.
    pub token_hash: String,
}

/// Generates an opaque refresh token together with its stored hash.
pub fn generate_refresh_token() -> IssuedRefreshToken {
    let mut bytes = [0u8; REFRESH_TOKEN_BYTES];
    rand::rng().fill_bytes(&mut bytes);
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let token_hash = hash_refresh_token(&token);
    IssuedRefreshToken { token, token_hash }
}

/// Computes the SHA-256 hash of a refresh token, hex encoded.
///
/// SHA-256 is appropriate here because the token is a high-entropy random
/// value, not a low-entropy password.
pub fn hash_refresh_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    // The digest is formatted as lower-case hexadecimal without a separator,
    // which is the encoding the stored hashes use.
    format!("{:x}", hasher.finalize())
}

/// Base64url engine without padding, used for opaque tokens.
use base64::Engine as _;

#[cfg(test)]
mod tests {
    use super::*;

    /// The clock-skew allowance the tests use.
    const CLOCK_SKEW: Duration = Duration::from_secs(30);

    /// The access-token lifetime the tests use.
    const ACCESS_TTL: Duration = Duration::from_secs(900);

    /// The subject the tests use.
    const USER: Uuid = Uuid::from_u128(0x0192_0000_0000_7000_8000_0000_0000_0001);

    fn service() -> TokenService {
        TokenService::new(
            "a-test-signing-secret-that-is-long-enough-32b",
            "atom-api",
            "atom-api",
            ACCESS_TTL,
            CLOCK_SKEW,
        )
    }

    #[test]
    fn issued_tokens_verify_and_carry_the_subject() {
        let service = service();
        let issued = service.issue_access_token(USER, "a@example.test").unwrap();
        let claims = service.decode_access_token(&issued.token).unwrap();
        assert_eq!(claims.sub, USER);
        assert_eq!(claims.email, "a@example.test");
        assert_eq!(claims.iss, "atom-api");
        assert_eq!(claims.aud, "atom-api");
    }

    #[test]
    fn a_token_signed_with_another_secret_is_rejected() {
        let issuer = service();
        let other = TokenService::new(
            "a-completely-different-secret-of-sufficient-length",
            "atom-api",
            "atom-api",
            ACCESS_TTL,
            CLOCK_SKEW,
        );
        let issued = other.issue_access_token(USER, "a@example.test").unwrap();
        assert!(issuer.decode_access_token(&issued.token).is_err());
    }

    #[test]
    fn a_token_from_another_issuer_is_rejected() {
        let issuer = service();
        let other = TokenService::new(
            "a-test-signing-secret-that-is-long-enough-32b",
            "somebody-else",
            "atom-api",
            ACCESS_TTL,
            CLOCK_SKEW,
        );
        let issued = other.issue_access_token(USER, "a@example.test").unwrap();
        assert!(issuer.decode_access_token(&issued.token).is_err());
    }

    #[test]
    fn garbage_tokens_are_rejected() {
        let service = service();
        assert!(service.decode_access_token("not.a.token").is_err());
        assert!(service.decode_access_token("").is_err());
    }

    #[test]
    fn every_failure_mode_is_the_same_error() {
        let service = service();
        let issued = service.issue_access_token(USER, "a@example.com").unwrap();

        // A token this service issued decodes.
        let claims = service
            .decode_access_token(&issued.token)
            .expect("a token this service issued must decode");
        assert_eq!(claims.sub, USER);

        // Every failure mode is the same error, so that a caller cannot tell
        // a forged token from an expired one.
        assert_eq!(
            service.decode_access_token("not.a.token").unwrap_err(),
            TokenError::Invalid
        );
        assert_eq!(
            service.decode_access_token("").unwrap_err(),
            TokenError::Invalid
        );

        // A token signed with another key is refused.
        let other = TokenService::new(
            "a-completely-different-secret-of-sufficient-length",
            "atom-api",
            "atom-api",
            ACCESS_TTL,
            CLOCK_SKEW,
        );
        let foreign = other.issue_access_token(USER, "a@example.com").unwrap();
        assert_eq!(
            service.decode_access_token(&foreign.token).unwrap_err(),
            TokenError::Invalid
        );
    }

    #[test]
    fn refresh_tokens_are_unique_and_only_the_hash_is_kept() {
        let first = generate_refresh_token();
        let second = generate_refresh_token();
        assert_ne!(first.token, second.token);
        assert_eq!(first.token_hash, hash_refresh_token(&first.token));
        assert!(!first.token_hash.contains(&first.token));
    }

    #[test]
    fn the_stored_hash_is_lower_case_hexadecimal() {
        // A literal, so that a change of the encoding cannot pass unnoticed:
        // the stored hashes are what the lookup matches against.
        assert_eq!(
            hash_refresh_token("atom"),
            "1bf521836de96e75fac7f9dedda4d92ebe4c01b987781e419ec0e421611a39d0"
        );
    }
}
