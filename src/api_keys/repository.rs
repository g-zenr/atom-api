//! API-key persistence.
//!
//! Every statement is scoped by `user_id` in its own `WHERE` clause, never by a
//! check in the handler (`ADR-0005`). A key belonging to another account is
//! therefore invisible to the repository, which is what makes the `404`-not-
//! `403` guarantee enforceable in one place.
//!
//! Exactly one statement reads `key_hash`, and it is the lookup a presented
//! credential goes through: [`FIND_KEY_IDENTITY`]. It **binds** a hash and
//! returns an identity, so there is no shape in this file that carries the hash
//! back out.

use sqlx::PgPool;
use uuid::Uuid;

use crate::api_keys::models::{ApiKey, KeyIdentity};
use crate::error::AppResult;

/// Inserts one key for the caller.
///
/// The secret itself is not a column: `key_hash` is the SHA-256 of it, and
/// `prefix` is the eight-character tag a listing shows. `created_at` takes the
/// column default, so the response publishes the stored value rather than a
/// value computed in Rust.
const INSERT_KEY: &str = r#"
    INSERT INTO api_keys (id, user_id, name, prefix, key_hash)
    VALUES ($1, $2, $3, $4, $5)
    RETURNING id, user_id, name, prefix, last_used_at, created_at, revoked_at
"#;

/// The caller's keys, newest first.
///
/// `api_keys_user_id_idx (user_id)` serves the predicate; the sort is on
/// `created_at DESC, id DESC` so that two keys created in the same transaction
/// clock still have a stable order and the pagination is not ambiguous.
const LIST_OWNED: &str = r#"
    SELECT id, user_id, name, prefix, last_used_at, created_at, revoked_at
    FROM api_keys
    WHERE user_id = $1
    ORDER BY created_at DESC, id DESC
    LIMIT $2 OFFSET $3
"#;

/// The count matching [`LIST_OWNED`], so the reported total is the size of the
/// set the page was taken from.
const COUNT_OWNED: &str = "SELECT COUNT(*) FROM api_keys WHERE user_id = $1";

/// Revokes one key of the caller, idempotently.
///
/// `COALESCE` keeps the **first** revocation instant: revoking twice is `204`
/// and does not rewrite when the credential was withdrawn. The statement is a
/// plain `UPDATE` rather than a guarded one on purpose — the route's contract
/// says a second `DELETE` succeeds, so the `WHERE` must match the row again.
const REVOKE_OWNED: &str = r#"
    UPDATE api_keys
    SET revoked_at = COALESCE(revoked_at, clock_timestamp())
    WHERE id = $1 AND user_id = $2
"#;

/// Resolves a presented secret to the account it acts for.
///
/// The `JOIN` is the reason this is one statement: an inactive account, a
/// revoked key and an unknown hash are all answered without a second round
/// trip, and no caller can be authenticated against a row whose account has
/// since been disabled.
///
/// The hash is a bound parameter. The value compared is `key_hash = $1`, an
/// equality on the unique constraint `api_keys_key_hash_key`, so the lookup is
/// an index probe and not a scan.
///
/// `k.last_used_at` is read for one purpose: the service skips the usage
/// statement when the stamp is still fresh. It is not part of the decision.
const FIND_KEY_IDENTITY: &str = r#"
    SELECT k.id AS key_id,
           k.user_id,
           u.email,
           u.is_active,
           k.revoked_at,
           k.last_used_at
    FROM api_keys AS k
    JOIN users AS u ON u.id = k.user_id
    WHERE k.key_hash = $1
"#;

/// Records that a key authenticated a request, at most once an hour.
///
/// The interesting half of the statement is the `AND`, not the `SET`: a hot key
/// — an n8n workflow on a one-minute schedule, say — writes at most 24 rows a
/// day instead of 1 440. The predicate is evaluated by the database, in the same
/// statement as the write, so two concurrent requests cannot both decide to
/// stamp.
///
/// `NOW()` is the transaction's start instant. It is the right clock here
/// because the stamp is approximate by design (hourly at best) and `NOW()` is
/// constant within the statement; the two-hour window that this could produce
/// around a transaction boundary is inside the tolerance the contract asks for.
const TOUCH_LAST_USED: &str = r#"
    UPDATE api_keys
    SET last_used_at = NOW()
    WHERE id = $1
      AND user_id = $2
      AND (last_used_at IS NULL OR last_used_at < NOW() - INTERVAL '1 hour')
"#;

/// Persistence operations for API keys.
#[derive(Debug, Clone)]
pub struct ApiKeyRepository {
    pool: PgPool,
}

impl ApiKeyRepository {
    /// Creates a repository bound to a connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Inserts one key and returns the stored row.
    pub async fn insert(
        &self,
        id: Uuid,
        owner: Uuid,
        name: &str,
        prefix: &str,
        key_hash: &str,
    ) -> AppResult<ApiKey> {
        let key = sqlx::query_as::<_, ApiKey>(INSERT_KEY)
            .bind(id)
            .bind(owner)
            .bind(name)
            .bind(prefix)
            .bind(key_hash)
            .fetch_one(&self.pool)
            .await?;

        Ok(key)
    }

    /// Lists the caller's keys, newest first, with the total of the collection.
    pub async fn list_owned(
        &self,
        owner: Uuid,
        limit: i64,
        offset: i64,
    ) -> AppResult<(Vec<ApiKey>, i64)> {
        let keys = sqlx::query_as::<_, ApiKey>(LIST_OWNED)
            .bind(owner)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        let total = sqlx::query_scalar::<_, i64>(COUNT_OWNED)
            .bind(owner)
            .fetch_one(&self.pool)
            .await?;

        Ok((keys, total))
    }

    /// Revokes a key of the caller. `false` means no key of that id belongs to
    /// this caller, which the service reports as a `404`.
    ///
    /// A key that is already revoked still matches, so a second call is `true`
    /// and the route answers `204`; see [`REVOKE_OWNED`].
    pub async fn revoke_owned(&self, key_id: Uuid, owner: Uuid) -> AppResult<bool> {
        let result = sqlx::query(REVOKE_OWNED)
            .bind(key_id)
            .bind(owner)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Resolves a stored hash to the account the key acts for.
    ///
    /// `None` means no key has that hash, which is the only thing this statement
    /// can say: whether the key is revoked and whether the account is active are
    /// columns of the returned row, and the service decides what they mean.
    pub async fn find_identity_by_hash(&self, key_hash: &str) -> AppResult<Option<KeyIdentity>> {
        let identity = sqlx::query_as::<_, KeyIdentity>(FIND_KEY_IDENTITY)
            .bind(key_hash)
            .fetch_optional(&self.pool)
            .await?;

        Ok(identity)
    }

    /// Stamps the key's last use, unless it was already stamped this hour.
    ///
    /// Returns whether a row was written, which is what a test asserts; the
    /// request path ignores it.
    pub async fn touch_last_used(&self, key_id: Uuid, owner: Uuid) -> AppResult<bool> {
        let result = sqlx::query(TOUCH_LAST_USED)
            .bind(key_id)
            .bind(owner)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}
