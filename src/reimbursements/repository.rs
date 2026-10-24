//! Claim persistence.
//!
//! Every statement is scoped by `user_id` in its own `WHERE` clause, never by
//! a check in the handler (`ADR-0005`). A claim belonging to another user is
//! therefore invisible to the repository, which is what makes the `404`-not-
//! `403` guarantee enforceable in one place.
//!
//! # Money
//!
//! Every money column is selected as `total_amount::text` / `amount::text` and
//! decodes into a [`String`]; see [`crate::reimbursements::models`]. The `AS`
//! alias is not strictly required today — PostgreSQL names a cast expression
//! after its column — but it makes the `FromRow` mapping independent of that
//! detail.
//!
//! # The project link
//!
//! `project_id` may only be set to a project of the caller. The predicate is
//! written **inside** the write statement as
//! `EXISTS (SELECT 1 FROM projects WHERE id = $n AND user_id = $m)`, so a
//! foreign project is a `404` without a second round trip and without a
//! check-then-act race. The composite key between the two tables cannot express
//! this (it is about an item and its claim, not a claim and a project), which
//! is recorded in `docs/database/schema.md` §11.1.

use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppResult;
use crate::reimbursements::models::{ClaimStatus, ExpenseClaim, ExpenseItem};

/// The filters applied to a claim listing.
///
/// A struct rather than eight positional arguments: an argument order that can
/// be silently swapped is a bug generator, and `clippy::too_many_arguments`
/// fires at eight. Every value has already been validated and normalised by
/// [`crate::reimbursements::service::ClaimService`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ClaimListFilter<'a> {
    /// Only claims in this status.
    pub status: Option<ClaimStatus>,
    /// Only claims in this currency, already trimmed and upper-cased.
    pub currency: Option<&'a str>,
    /// Substring of the title or the description, already trimmed and
    /// non-empty.
    pub search: Option<&'a str>,
    /// Inclusive lower bound on `submitted_at`.
    pub submitted_from: Option<NaiveDate>,
    /// Inclusive upper bound on `submitted_at`.
    pub submitted_to: Option<NaiveDate>,
}

/// Creates a draft claim, provided the project link (if any) is the caller's.
///
/// The ownership predicate for the project lives in this statement: the
/// `SELECT` yields one row or none, and `None` therefore means exactly "the
/// project does not exist or is not the caller's". Nothing else can suppress
/// the row — the id is freshly generated, the owner comes from a valid token
/// and the text fields were validated before the statement runs — so the
/// service maps `None` to the project's `404` without a second query.
const INSERT_CLAIM: &str = r#"
    INSERT INTO expense_claims (id, user_id, title, description, currency, status, project_id)
    SELECT $1, $2, $3, $4, $5::char(3), 'draft'::expense_claim_status, $6::uuid
    WHERE $6::uuid IS NULL
       OR EXISTS (SELECT 1 FROM projects WHERE id = $6::uuid AND user_id = $2)
    RETURNING id, user_id, title, description, currency,
              total_amount::text AS total_amount, status, project_id,
              submitted_at, decided_at, paid_at, created_at, updated_at
"#;

/// Reads one claim of the caller.
const FIND_CLAIM_OWNED: &str = r#"
    SELECT id, user_id, title, description, currency,
           total_amount::text AS total_amount, status, project_id,
           submitted_at, decided_at, paid_at, created_at, updated_at
    FROM expense_claims
    WHERE id = $1 AND user_id = $2
"#;

/// Reads the items of one claim of the caller, in the published order.
///
/// Both predicates are present: the claim id and the owner. An item id alone is
/// never enough to address a row, and pairing the claim id with the owner means
/// the statement cannot return another account's items even if the claim lookup
/// that precedes it ever changed.
const LIST_ITEMS_OWNED: &str = r#"
    SELECT id, user_id, claim_id, description,
           amount::text AS amount, incurred_on, category, receipt_url,
           created_at, updated_at
    FROM expense_items
    WHERE claim_id = $1 AND user_id = $2
    ORDER BY incurred_on, created_at, id
"#;

/// The page statement for a listing without a status filter.
///
/// `expense_claims_user_id_created_at_idx (user_id, created_at DESC)` serves it:
/// equality on the leading column, and the order matches the index direction.
///
/// Every optional parameter carries an explicit cast. A parameter that appears
/// only as `$n IS NULL` has no expression from which PostgreSQL could infer its
/// type, so the cast is what makes the statement's parameter types definite.
///
/// The date bounds are UTC days, half-open at the top: `>= from 00:00:00Z` and
/// `< (to + 1) day 00:00:00Z` is "inclusive of both ends" without a
/// `23:59:59.999999` literal, and it does not depend on the session
/// `TimeZone`, which a bare `date::timestamptz` would. A draft has a `NULL`
/// `submitted_at`; both comparisons are then `NULL` and the row drops out,
/// which is the published behaviour when either bound is given.
const PAGE_WITHOUT_STATUS: &str = r#"
    SELECT id, user_id, title, description, currency,
           total_amount::text AS total_amount, status, project_id,
           submitted_at, decided_at, paid_at, created_at, updated_at
    FROM expense_claims
    WHERE user_id = $1
      AND ($2::char(3) IS NULL OR currency = $2::char(3))
      AND ($3::date IS NULL OR submitted_at >= ($3::date::timestamp AT TIME ZONE 'UTC'))
      AND ($4::date IS NULL OR submitted_at <  (($4::date + 1)::timestamp AT TIME ZONE 'UTC'))
      AND ($5::text IS NULL
           OR title ILIKE '%' || $5 || '%'
           OR description ILIKE '%' || $5 || '%')
    ORDER BY created_at DESC, id DESC
    LIMIT $6 OFFSET $7
"#;

/// The page statement for a listing filtered by status.
///
/// The second listing index,
/// `expense_claims_user_id_status_created_at_idx (user_id, status, created_at
/// DESC)`, serves it. A single statement that wrote `($2 IS NULL OR status =
/// $2)` would hide the status from the planner and leave that index unread, so
/// the repository holds two shapes instead. Every other clause is identical.
const PAGE_WITH_STATUS: &str = r#"
    SELECT id, user_id, title, description, currency,
           total_amount::text AS total_amount, status, project_id,
           submitted_at, decided_at, paid_at, created_at, updated_at
    FROM expense_claims
    WHERE user_id = $1
      AND status = $2::expense_claim_status
      AND ($3::char(3) IS NULL OR currency = $3::char(3))
      AND ($4::date IS NULL OR submitted_at >= ($4::date::timestamp AT TIME ZONE 'UTC'))
      AND ($5::date IS NULL OR submitted_at <  (($5::date + 1)::timestamp AT TIME ZONE 'UTC'))
      AND ($6::text IS NULL
           OR title ILIKE '%' || $6 || '%'
           OR description ILIKE '%' || $6 || '%')
    ORDER BY created_at DESC, id DESC
    LIMIT $7 OFFSET $8
"#;

/// The count statement matching [`PAGE_WITHOUT_STATUS`].
///
/// The filter is the same filter as in the page statement, so the reported
/// total is the size of the set the page was taken from — never the whole
/// collection.
const COUNT_WITHOUT_STATUS: &str = r#"
    SELECT COUNT(*)
    FROM expense_claims
    WHERE user_id = $1
      AND ($2::char(3) IS NULL OR currency = $2::char(3))
      AND ($3::date IS NULL OR submitted_at >= ($3::date::timestamp AT TIME ZONE 'UTC'))
      AND ($4::date IS NULL OR submitted_at <  (($4::date + 1)::timestamp AT TIME ZONE 'UTC'))
      AND ($5::text IS NULL
           OR title ILIKE '%' || $5 || '%'
           OR description ILIKE '%' || $5 || '%')
"#;

/// The count statement matching [`PAGE_WITH_STATUS`].
const COUNT_WITH_STATUS: &str = r#"
    SELECT COUNT(*)
    FROM expense_claims
    WHERE user_id = $1
      AND status = $2::expense_claim_status
      AND ($3::char(3) IS NULL OR currency = $3::char(3))
      AND ($4::date IS NULL OR submitted_at >= ($4::date::timestamp AT TIME ZONE 'UTC'))
      AND ($5::date IS NULL OR submitted_at <  (($5::date + 1)::timestamp AT TIME ZONE 'UTC'))
      AND ($6::text IS NULL
           OR title ILIKE '%' || $6 || '%'
           OR description ILIKE '%' || $6 || '%')
"#;

/// Applies a partial update, with the project rule in the statement.
///
/// Binds are `claim_id`, `owner`, `title`, `description`, `clear_project`,
/// `project_id` — exactly the six the tri-state of
/// [`crate::reimbursements::dto::UpdateClaimRequest`] resolves to.
///
/// `COALESCE($n, column)` is the partial-update rule made mechanical: `NULL` in
/// the binds means "keep", so a handler that forgot a field cannot null it. The
/// boolean is needed because a `NULL` project id already means "leave alone";
/// only the boolean can say "clear". `currency`, `status` and the lifecycle
/// timestamps are not in the `SET` clause, so no `PATCH` can write them.
const UPDATE_CLAIM_OWNED: &str = r#"
    UPDATE expense_claims
    SET title       = COALESCE($3, title),
        description = COALESCE($4, description),
        project_id  = CASE WHEN $5 THEN NULL ELSE COALESCE($6::uuid, project_id) END,
        updated_at  = NOW()
    WHERE id = $1
      AND user_id = $2
      AND ($6::uuid IS NULL
           OR EXISTS (SELECT 1 FROM projects WHERE id = $6::uuid AND user_id = $2))
    RETURNING id, user_id, title, description, currency,
              total_amount::text AS total_amount, status, project_id,
              submitted_at, decided_at, paid_at, created_at, updated_at
"#;

/// Deletes one claim of the caller.
///
/// The items go with it through `expense_items_claim_fk … ON DELETE CASCADE`,
/// in the same statement. No transaction is needed because there is exactly one
/// statement.
const DELETE_CLAIM_OWNED: &str = "DELETE FROM expense_claims WHERE id = $1 AND user_id = $2";

/// Persistence operations for expense claims.
#[derive(Debug, Clone)]
pub struct ClaimRepository {
    pool: PgPool,
}

impl ClaimRepository {
    /// Creates a repository bound to a connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Inserts a draft claim.
    ///
    /// Returns `None` when `project_id` is set and no project of that id
    /// belongs to `owner`; the service maps that to the project's `404`.
    pub async fn insert(
        &self,
        id: Uuid,
        owner: Uuid,
        title: &str,
        description: &str,
        currency: &str,
        project_id: Option<Uuid>,
    ) -> AppResult<Option<ExpenseClaim>> {
        let claim = sqlx::query_as::<_, ExpenseClaim>(INSERT_CLAIM)
            .bind(id)
            .bind(owner)
            .bind(title)
            .bind(description)
            .bind(currency)
            .bind(project_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(claim)
    }

    /// Finds a claim owned by `owner`.
    pub async fn find_owned(&self, claim_id: Uuid, owner: Uuid) -> AppResult<Option<ExpenseClaim>> {
        let claim = sqlx::query_as::<_, ExpenseClaim>(FIND_CLAIM_OWNED)
            .bind(claim_id)
            .bind(owner)
            .fetch_optional(&self.pool)
            .await?;

        Ok(claim)
    }

    /// Lists the items of a claim owned by `owner`.
    ///
    /// This is the detail read, not an item endpoint: `R2` reads items because
    /// the published detail response contains them, and writes none.
    pub async fn list_items_owned(
        &self,
        claim_id: Uuid,
        owner: Uuid,
    ) -> AppResult<Vec<ExpenseItem>> {
        let items = sqlx::query_as::<_, ExpenseItem>(LIST_ITEMS_OWNED)
            .bind(claim_id)
            .bind(owner)
            .fetch_all(&self.pool)
            .await?;

        Ok(items)
    }

    /// Lists the claims owned by `owner`, with the total of the filtered set.
    ///
    /// Two statement shapes and two counts, one pair per listing index. The
    /// statement that runs depends on whether a status filter was given; see
    /// [`PAGE_WITHOUT_STATUS`] and [`PAGE_WITH_STATUS`] for why.
    pub async fn list_owned(
        &self,
        owner: Uuid,
        filter: &ClaimListFilter<'_>,
        limit: i64,
        offset: i64,
    ) -> AppResult<(Vec<ExpenseClaim>, i64)> {
        let (items, total) = match filter.status {
            Some(status) => {
                let items = sqlx::query_as::<_, ExpenseClaim>(PAGE_WITH_STATUS)
                    .bind(owner)
                    .bind(status)
                    .bind(filter.currency)
                    .bind(filter.submitted_from)
                    .bind(filter.submitted_to)
                    .bind(filter.search)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(&self.pool)
                    .await?;

                let total = sqlx::query_scalar::<_, i64>(COUNT_WITH_STATUS)
                    .bind(owner)
                    .bind(status)
                    .bind(filter.currency)
                    .bind(filter.submitted_from)
                    .bind(filter.submitted_to)
                    .bind(filter.search)
                    .fetch_one(&self.pool)
                    .await?;

                (items, total)
            }
            None => {
                let items = sqlx::query_as::<_, ExpenseClaim>(PAGE_WITHOUT_STATUS)
                    .bind(owner)
                    .bind(filter.currency)
                    .bind(filter.submitted_from)
                    .bind(filter.submitted_to)
                    .bind(filter.search)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(&self.pool)
                    .await?;

                let total = sqlx::query_scalar::<_, i64>(COUNT_WITHOUT_STATUS)
                    .bind(owner)
                    .bind(filter.currency)
                    .bind(filter.submitted_from)
                    .bind(filter.submitted_to)
                    .bind(filter.search)
                    .fetch_one(&self.pool)
                    .await?;

                (items, total)
            }
        };

        Ok((items, total))
    }

    /// Applies a partial update to a claim owned by `owner`.
    ///
    /// Returns `None` when either the claim is absent or foreign **or** the
    /// project is; the service disambiguates on its error path with one
    /// [`Self::find_owned`].
    pub async fn update_owned(
        &self,
        claim_id: Uuid,
        owner: Uuid,
        title: Option<&str>,
        description: Option<&str>,
        clear_project: bool,
        project_id: Option<Uuid>,
    ) -> AppResult<Option<ExpenseClaim>> {
        let claim = sqlx::query_as::<_, ExpenseClaim>(UPDATE_CLAIM_OWNED)
            .bind(claim_id)
            .bind(owner)
            .bind(title)
            .bind(description)
            .bind(clear_project)
            .bind(project_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(claim)
    }

    /// Deletes a claim owned by `owner`. `false` means it does not exist for
    /// this caller.
    pub async fn delete_owned(&self, claim_id: Uuid, owner: Uuid) -> AppResult<bool> {
        let result = sqlx::query(DELETE_CLAIM_OWNED)
            .bind(claim_id)
            .bind(owner)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}
