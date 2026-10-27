//! Public contracts for the `api_keys` module.
//!
//! Three response types and one request type, field for field as the locked
//! contract publishes them:
//!
//! * [`ApiKeyCreatedResponse`] is the **only** shape that carries the secret,
//!   and it is returned by exactly one operation. There is no second type with
//!   a `key` field, so a listing cannot accidentally serialize one.
//! * [`ApiKeyResponse`] is the listing shape: id, name, prefix, usage and the
//!   two timestamps. The prefix is enough to recognise a key and far too little
//!   to use it.
//! * [`ApiKeyPage`] is the paged envelope, `items` plus `pagination`, the same
//!   envelope every other list endpoint returns.
//!
//! A response is built from [`ApiKey`](crate::api_keys::models::ApiKey), which
//! does not carry the stored hash; see the module documentation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api_keys::models::ApiKey;
use crate::pagination::{Page, Pagination};

/// Maximum accepted key-name length.
///
/// Mirrors `api_keys_name_length_chk`
/// (`CHECK (CHAR_LENGTH(name) BETWEEN 1 AND 100)`) in
/// `migrations/0007_api_keys.sql`. The migration is the enforced copy; this
/// constant is the application-side mirror of it, so a refusal is a `422`
/// naming `name` rather than a constraint violation.
pub const MAX_KEY_NAME_LENGTH: usize = 100;

/// The body of `POST /api/v1/api-keys`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateApiKeyRequest {
    /// A label for the operator, for example `n8n` or `home-assistant`.
    ///
    /// Trimmed before it is measured and stored: a name of spaces is empty, and
    /// empty is a `422` naming `name`.
    pub name: String,
}

/// The body of `POST /api/v1/api-keys` — the one response that carries the
/// secret.
///
/// `key` is returned here and **never again**: no read, no list and no error
/// contains it, and the database stores only its SHA-256. A client that loses it
/// revokes the key and creates another.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ApiKeyCreatedResponse {
    /// Stable public identifier, used to revoke the key.
    pub id: Uuid,
    /// The stored label.
    pub name: String,
    /// The secret, `atom_` followed by 64 lower-case hex characters.
    ///
    /// Shown once. It is a credential: do not log it, and do not put it in a
    /// URL.
    #[schema(example = "atom_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")]
    pub key: String,
    /// `atom_` plus the first eight hex characters of the random part.
    pub prefix: String,
    /// Creation timestamp, RFC 3339 UTC.
    pub created_at: DateTime<Utc>,
}

/// One key in a listing. Never carries the secret, and never the hash.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ApiKeyResponse {
    /// Stable public identifier, used to revoke the key.
    pub id: Uuid,
    /// The stored label.
    pub name: String,
    /// `atom_` plus eight hex characters: enough to recognise, too little to use.
    pub prefix: String,
    /// When the key last authenticated a request.
    ///
    /// Recorded at most once an hour per key, so it is the time of the first
    /// authenticated request of the current hour rather than of the latest one.
    /// `null` until the key is first used.
    pub last_used_at: Option<DateTime<Utc>>,
    /// Creation timestamp, RFC 3339 UTC.
    pub created_at: DateTime<Utc>,
    /// The revocation instant, or `null` while the key is live.
    ///
    /// A revoked key stays in the listing: the contract has no hard delete, and
    /// an operator needs to see that a credential was withdrawn and when.
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Builds the listing representation of a key. `user_id` is dropped.
impl From<ApiKey> for ApiKeyResponse {
    fn from(key: ApiKey) -> Self {
        Self {
            id: key.id,
            name: key.name,
            prefix: key.prefix,
            last_used_at: key.last_used_at,
            created_at: key.created_at,
            revoked_at: key.revoked_at,
        }
    }
}

/// The response body of `GET /api/v1/api-keys`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ApiKeyPage {
    /// The keys on this page, newest first.
    pub items: Vec<ApiKeyResponse>,
    /// Pagination metadata.
    pub pagination: Pagination,
}

impl From<Page<ApiKeyResponse>> for ApiKeyPage {
    fn from(page: Page<ApiKeyResponse>) -> Self {
        Self {
            items: page.items,
            pagination: page.pagination,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> ApiKey {
        ApiKey {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
            name: "n8n".to_string(),
            prefix: "atom_01234567".to_string(),
            last_used_at: None,
            created_at: Utc::now(),
            revoked_at: None,
        }
    }

    /// The listing shape has no `key` and no `user_id` on the wire.
    ///
    /// Serializing the type is the honest test: a field added to the struct
    /// appears in the JSON, so this fails if one is added.
    #[test]
    fn the_listing_representation_carries_no_secret_and_no_owner() {
        let response = ApiKeyResponse::from(row());
        let json = serde_json::to_value(&response).expect("the listing serializes");
        let object = json.as_object().expect("the listing is an object");

        let mut names: Vec<&str> = object.keys().map(String::as_str).collect();
        names.sort_unstable();

        assert_eq!(
            names,
            [
                "created_at",
                "id",
                "last_used_at",
                "name",
                "prefix",
                "revoked_at"
            ]
        );
        assert!(
            !json.to_string().contains("key_hash"),
            "the hash must never reach the wire"
        );
    }
}
