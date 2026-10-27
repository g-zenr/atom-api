//! API-key use cases: creation, listing, revocation and authentication.
//!
//! Every method takes `owner` first, exactly as `ClaimService` does, so a key
//! operation cannot be called without saying whose key is meant. The service
//! never compares identifiers and never answers `403` for an owner mismatch: a
//! repository miss becomes [`AppError::NotFound`] with the same message for
//! "does not exist" and "belongs to somebody else" (`ADR-0005` §3).
//!
//! Three rules hold in this order:
//!
//! 1. **Validate the body before the statement runs**, so a malformed request
//!    is a `422` whatever it was aimed at.
//! 2. **Every miss is a catalogue message**, never a message assembled here.
//! 3. **A credential failure names the right subject.** A key that is unknown,
//!    malformed or revoked is a `401`, because the credential is the problem. A
//!    key whose *account* is disabled is a `403 messages.auth.account_disabled`,
//!    because the credential is fine and the account is not — the same answer
//!    `AuthService::authenticate_bearer` gives for a JWT of that account
//!    (amendment `A-C-K1-1`). Collapsing the two would give one account state
//!    two different answers depending on which credential was presented.

use std::sync::Arc;

use uuid::Uuid;

use crate::api_keys::dto::{
    ApiKeyCreatedResponse, ApiKeyResponse, CreateApiKeyRequest, MAX_KEY_NAME_LENGTH,
};
use crate::api_keys::models::KeyIdentity;
use crate::api_keys::repository::ApiKeyRepository;
use crate::api_keys::secret::{self, GeneratedKey};
use crate::config::messages::Messages;
use crate::config::settings::PaginationSettings;
use crate::error::{AppError, AppResult};
use crate::helpers::required_text;
use crate::pagination::{Page, PaginationQuery};

/// The caller an API key authenticates as.
///
/// A value of this type means the credential was accepted: the key exists, it is
/// not revoked and its account is active. The middleware turns it into an
/// [`AuthUser`](crate::api::middleware::AuthUser) and then the request runs
/// exactly as if it had arrived with a JWT — including the key-management
/// endpoints, so a key can rotate itself.
#[derive(Debug, Clone)]
pub struct KeyPrincipal {
    /// The account the key acts for.
    pub user_id: Uuid,
    /// The account's address, carried so a handler can report it.
    pub email: String,
}

/// API-key use cases.
#[derive(Debug, Clone)]
pub struct ApiKeyService {
    repository: ApiKeyRepository,
    messages: Arc<Messages>,
    pagination: PaginationSettings,
}

impl ApiKeyService {
    /// Builds the service.
    pub fn new(
        repository: ApiKeyRepository,
        messages: Arc<Messages>,
        pagination: PaginationSettings,
    ) -> Self {
        Self {
            repository,
            messages,
            pagination,
        }
    }

    /// Creates a key for `owner` and returns the secret, once.
    ///
    /// The secret is generated here and leaves this function in exactly one
    /// place: the [`ApiKeyCreatedResponse`]. The repository receives its hash,
    /// so the plaintext is not written anywhere, and no later read can produce
    /// it again.
    pub async fn create(
        &self,
        owner: Uuid,
        request: CreateApiKeyRequest,
    ) -> AppResult<ApiKeyCreatedResponse> {
        let name = validate_name(&request.name, &self.messages)?;

        let GeneratedKey {
            key,
            prefix,
            key_hash,
        } = secret::generate();

        let stored = self
            .repository
            .insert(Uuid::new_v4(), owner, &name, &prefix, &key_hash)
            .await?;

        Ok(ApiKeyCreatedResponse {
            id: stored.id,
            name: stored.name,
            key,
            prefix: stored.prefix,
            created_at: stored.created_at,
        })
    }

    /// Lists the caller's keys, newest first.
    ///
    /// The listing contains every key, revoked ones included: the contract has
    /// no hard delete, and an operator needs to see that a credential was
    /// withdrawn. No response here can carry a secret — see
    /// [`ApiKeyResponse`].
    pub async fn list(
        &self,
        owner: Uuid,
        pagination: PaginationQuery,
    ) -> AppResult<Page<ApiKeyResponse>> {
        let limit = pagination.effective_limit(&self.pagination);
        let offset = pagination.effective_offset();

        let (keys, total) = self.repository.list_owned(owner, limit, offset).await?;

        let items = keys.into_iter().map(ApiKeyResponse::from).collect();

        Ok(Page::new(items, limit, offset, total))
    }

    /// Revokes one of the caller's keys.
    ///
    /// Revoking twice is not an error: the statement matches the row the second
    /// time as well and keeps the first instant, so the route answers `204`
    /// both times. A key of another account is indistinguishable from one that
    /// does not exist.
    pub async fn revoke(&self, owner: Uuid, key_id: Uuid) -> AppResult<()> {
        let revoked = self.repository.revoke_owned(key_id, owner).await?;

        if !revoked {
            return Err(AppError::NotFound(
                self.messages.resources.api_key_not_found.clone(),
            ));
        }

        Ok(())
    }

    /// Authenticates a presented secret.
    ///
    /// The order is deliberate:
    ///
    /// 1. **Shape.** A value that is not `atom_` plus 64 lower-case hex
    ///    characters cannot match any stored hash, so it is refused before a
    ///    statement runs — a garbage `atom_…` token is a `401`, never a `500`,
    ///    and never a database round trip.
    /// 2. **Lookup.** One statement, by hash, joined to `users`.
    /// 3. **State.** [`identity_refusal`] turns the row into the published
    ///    answer: the key's own state is a `401`, the account's is a `403`.
    /// 4. **Usage.** The stamp is recorded through one guarded `UPDATE`, and a
    ///    failure there is logged rather than reported.
    ///
    /// A failed usage stamp does not fail the request: recording when a
    /// credential was last used is telemetry, not a precondition. Turning a
    /// valid key into a `500` because a bookkeeping write failed would be a
    /// worse outcome than an hour that is missing from `last_used_at`.
    pub async fn authenticate(&self, presented: &str) -> AppResult<KeyPrincipal> {
        if !secret::is_well_formed(presented) {
            return Err(self.invalid_key());
        }

        let Some(identity) = self
            .repository
            .find_identity_by_hash(&secret::hash(presented))
            .await?
        else {
            return Err(self.invalid_key());
        };

        if let Some(refusal) = identity_refusal(&identity, &self.messages) {
            return Err(refusal);
        }

        self.record_use(&identity).await;

        Ok(KeyPrincipal {
            user_id: identity.user_id,
            email: identity.email,
        })
    }

    /// Stamps the key's last use, logging a failure instead of reporting it.
    ///
    /// The statement is skipped when the stored stamp is still inside the hour,
    /// so a hot key costs one read and no write; see
    /// [`KeyIdentity::needs_usage_stamp`] for why the skip cannot weaken the
    /// throttle, which the statement enforces.
    ///
    /// Nothing derived from the secret is logged: the message names the key id
    /// and the error, and the error is an [`AppError`], which redacts database
    /// detail.
    async fn record_use(&self, identity: &KeyIdentity) {
        if !identity.needs_usage_stamp(chrono::Utc::now()) {
            return;
        }

        if let Err(error) = self
            .repository
            .touch_last_used(identity.key_id, identity.user_id)
            .await
        {
            tracing::warn!(
                key_id = %identity.key_id,
                error = %error,
                "could not record an API key's last use"
            );
        }
    }

    /// The one answer for a credential that is not accepted.
    ///
    /// The wording is [`AuthMessages::access_token_invalid`], the same sentence
    /// the JWT path answers: one rejected bearer credential, one message. The
    /// `403` of a disabled account is **not** built here — that is a different
    /// subject, and it is decided in [`identity_refusal`].
    ///
    /// [`AuthMessages::access_token_invalid`]: crate::config::messages::AuthMessages::access_token_invalid
    fn invalid_key(&self) -> AppError {
        AppError::Unauthorized(self.messages.auth.access_token_invalid.clone())
    }
}

/// Refuses an identity that must not authenticate, or allows it.
///
/// A free function rather than a method: it reads nothing but the row and the
/// catalogue, so all three of its outcomes are unit-tested here — without a
/// pool, a runtime or a database. [`ApiKeyService::authenticate`] is the only
/// caller, and this is the **single** definition of the rule.
///
/// The order puts the credential before the account: a revoked key of a
/// disabled account is `401`, because the credential is the first thing that is
/// wrong and "get a new credential" is the remedy that changes nothing else.
pub fn identity_refusal(identity: &KeyIdentity, messages: &Messages) -> Option<AppError> {
    if identity.revoked_at.is_some() {
        return Some(AppError::Unauthorized(
            messages.auth.access_token_invalid.clone(),
        ));
    }

    if !identity.is_active {
        // The same status and the same message the JWT path answers for this
        // account: one account state, one answer (amendment `A-C-K1-1`).
        return Some(AppError::Forbidden(messages.auth.account_disabled.clone()));
    }

    None
}

/// Validates a key name.
///
/// The name is trimmed before it is measured, so 100 characters surrounded by
/// spaces is accepted and stored trimmed, and a name that is empty after
/// trimming is a `422` naming `name`. `required_text` is the shared helper, so
/// the order of the two checks and the two messages are the ones every other
/// title and name uses.
pub fn validate_name(name: &str, messages: &Messages) -> AppResult<String> {
    required_text(
        "name",
        name,
        MAX_KEY_NAME_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::messages::test_messages;
    use crate::error::code;
    use chrono::Utc;

    fn identity(revoked_at: Option<chrono::DateTime<Utc>>, is_active: bool) -> KeyIdentity {
        KeyIdentity {
            key_id: Uuid::nil(),
            user_id: Uuid::nil(),
            email: "owner@atom.local".to_string(),
            is_active,
            revoked_at,
            last_used_at: None,
        }
    }

    /// `A-C-K1-1`, the whole rule in one test: the credential's state is a
    /// `401` and the account's state is a `403` carrying the same message the
    /// JWT path uses. A single combined rule would answer `401` here and give a
    /// disabled account two different answers depending on the credential.
    #[test]
    fn a_revoked_key_is_unauthorized_and_a_disabled_account_is_forbidden() {
        let messages = test_messages();

        assert!(
            identity_refusal(&identity(None, true), &messages).is_none(),
            "a live key of an active account must authenticate"
        );

        let revoked = identity_refusal(&identity(Some(Utc::now()), true), &messages)
            .expect("a revoked key must be refused");
        assert_eq!(revoked.status(), axum::http::StatusCode::UNAUTHORIZED);
        assert_eq!(revoked.body().error.code, code::UNAUTHORIZED);
        assert_eq!(
            revoked.body().error.message,
            messages.auth.access_token_invalid
        );

        let disabled = identity_refusal(&identity(None, false), &messages)
            .expect("a key of a disabled account must be refused");
        assert_eq!(disabled.status(), axum::http::StatusCode::FORBIDDEN);
        assert_eq!(disabled.body().error.code, code::FORBIDDEN);
        assert_eq!(
            disabled.body().error.message,
            messages.auth.account_disabled
        );

        // Both wrong: the credential is checked first, so the answer is the
        // `401` that a new key would fix.
        let both = identity_refusal(&identity(Some(Utc::now()), false), &messages)
            .expect("a revoked key of a disabled account must be refused");
        assert_eq!(both.status(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn a_key_name_is_trimmed_before_it_is_measured() {
        let messages = test_messages();
        let name = "x".repeat(MAX_KEY_NAME_LENGTH);

        assert_eq!(
            validate_name(&format!("   {name}   "), &messages).expect("a name at the bound"),
            name,
            "the surrounding spaces are not counted and are not stored"
        );

        // One character past the bound, after trimming, is refused.
        let error = validate_name(
            &format!("  {}  ", "x".repeat(MAX_KEY_NAME_LENGTH + 1)),
            &messages,
        )
        .expect_err("an over-long name must be refused");
        assert_eq!(error.status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(error.body().error.code, code::VALIDATION_FAILED);
        assert_eq!(error.body().error.details[0].field, "name");
    }

    #[test]
    fn an_empty_key_name_is_refused() {
        let messages = test_messages();

        for refused in ["", "   ", "\t"] {
            let error = validate_name(refused, &messages)
                .expect_err("an empty name after trimming must be refused");
            assert_eq!(
                error.body().error.details[0].field,
                "name",
                "{refused:?} must be reported on `name`"
            );
            assert_eq!(
                error.body().error.details[0].message,
                messages.validation.not_empty,
                "{refused:?} must carry the empty-value message"
            );
        }
    }

    /// A key name is not a credential path, so a name of spaces is the only
    /// refused empty form. This pins the trimming rule at one character.
    #[test]
    fn a_single_character_key_name_is_accepted() {
        let messages = test_messages();

        assert_eq!(validate_name(" n ", &messages).expect("one character"), "n");
    }
}
