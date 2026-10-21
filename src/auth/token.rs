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

use crate::error::{AppError, AppResult};

/// Number of random bytes in a refresh token before encoding.
const REFRESH_TOKEN_BYTES: usize = 32;

/// Claims carried by an ATOM access token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    /// Subject: the user id.
    pub sub: String,
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
}

impl std::fmt::Debug for TokenService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenService")
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("access_ttl", &self.access_ttl)
            .finish_non_exhaustive()
    }
}

impl TokenService {
    /// Builds a token service from configuration.
    pub fn new(secret: &str, issuer: &str, audience: &str, access_ttl: Duration) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            issuer: issuer.to_string(),
            audience: audience.to_string(),
            access_ttl,
        }
    }

    /// Issues an access token for a user.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Internal`] when the token cannot be signed.
    pub fn issue_access_token(&self, user_id: &str, email: &str) -> AppResult<IssuedAccessToken> {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::from_std(self.access_ttl).unwrap_or_default();

        let claims = AccessTokenClaims {
            sub: user_id.to_string(),
            email: email.to_string(),
            iss: self.issuer.clone(),
            aud: self.audience.clone(),
            exp: expires_at.timestamp(),
            iat: now.timestamp(),
            jti: uuid::Uuid::new_v4().to_string(),
        };

        let token = encode(&Header::new(Algorithm::HS256), &claims, &self.encoding_key)
            .map_err(|error| AppError::Internal(format!("could not sign token: {error}")))?;

        Ok(IssuedAccessToken { token, expires_at })
    }

    /// Verifies an access token and returns its claims.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unauthorized`] for every failure mode (bad
    /// signature, expiry, wrong issuer or audience). The caller must not be
    /// able to distinguish between them.
    pub fn decode_access_token(&self, token: &str) -> AppResult<AccessTokenClaims> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.set_audience(&[self.audience.as_str()]);
        validation.set_required_spec_claims(&["exp", "sub", "iss", "aud"]);
        // Small clock-skew allowance for distributed deployments.
        validation.leeway = 30;

        decode::<AccessTokenClaims>(token, &self.decoding_key, &validation)
            .map(|data| data.claims)
            .map_err(|_| AppError::Unauthorized("The access token is invalid or expired."))
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
    hex::encode(hasher.finalize())
}

/// Base64url engine without padding, used for opaque tokens.
use base64::Engine as _;

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> TokenService {
        TokenService::new(
            "a-test-signing-secret-that-is-long-enough-32b",
            "atom-api",
            "atom-api",
            Duration::from_secs(900),
        )
    }

    #[test]
    fn issued_tokens_verify_and_carry_the_subject() {
        let service = service();
        let issued = service
            .issue_access_token("user-1", "a@example.test")
            .unwrap();
        let claims = service.decode_access_token(&issued.token).unwrap();
        assert_eq!(claims.sub, "user-1");
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
            Duration::from_secs(900),
        );
        let issued = other
            .issue_access_token("user-1", "a@example.test")
            .unwrap();
        assert!(issuer.decode_access_token(&issued.token).is_err());
    }

    #[test]
    fn a_token_from_another_issuer_is_rejected() {
        let issuer = service();
        let other = TokenService::new(
            "a-test-signing-secret-that-is-long-enough-32b",
            "somebody-else",
            "atom-api",
            Duration::from_secs(900),
        );
        let issued = other
            .issue_access_token("user-1", "a@example.test")
            .unwrap();
        assert!(issuer.decode_access_token(&issued.token).is_err());
    }

    #[test]
    fn garbage_tokens_are_rejected() {
        let service = service();
        assert!(service.decode_access_token("not.a.token").is_err());
        assert!(service.decode_access_token("").is_err());
    }

    #[test]
    fn refresh_tokens_are_unique_and_only_the_hash_is_kept() {
        let first = generate_refresh_token();
        let second = generate_refresh_token();
        assert_ne!(first.token, second.token);
        assert_eq!(first.token_hash, hash_refresh_token(&first.token));
        assert!(!first.token_hash.contains(&first.token));
    }
}
