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
use crate::reimbursements::service::LifecycleAction;

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

/// Runs the two reads of the detail response against one snapshot.
///
/// `get`, `update` and every lifecycle action return a claim **and** its items.
/// Under the default `READ COMMITTED`, those were two statements and therefore
/// two snapshots: an item committed between them would be listed next to a
/// `total_amount` that predates it, so the response would publish exactly the
/// divergence the whole transaction exists to prevent. `REPEATABLE READ` gives
/// both reads the transaction's first snapshot, so the total and the items in
/// one response always agree.
///
/// The transaction is read only, so the isolation level costs no lock.
const SET_READ_SNAPSHOT: &str = "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY";

/// Locks one claim of the caller and reports its status.
///
/// `FOR UPDATE` is what makes the item write safe: every item writer takes this
/// lock first, so two writers on one claim are serialised and the `SUM` each of
/// them computes in step 2 sees the other's committed row. Without it, two
/// concurrent inserts could each compute a total that misses the other.
const LOCK_CLAIM: &str =
    "SELECT status FROM expense_claims WHERE id = $1 AND user_id = $2 FOR UPDATE";

/// Inserts one item of the caller's claim.
///
/// `user_id` is written from the caller and never from the body, and the
/// composite key `expense_items_claim_fk` independently refuses a row whose
/// owner is not its claim's owner.
const INSERT_ITEM: &str = r#"
    INSERT INTO expense_items
        (id, user_id, claim_id, description, amount, incurred_on, category, receipt_url)
    VALUES ($1, $2, $3, $4, $5::numeric, $6::date, $7, $8)
    RETURNING id, user_id, claim_id, description,
              amount::text AS amount, incurred_on, category, receipt_url,
              created_at, updated_at
"#;

/// Applies a partial update to one item of the caller's claim.
///
/// Binds are `item_id`, `claim_id`, `owner`, `description`, `amount`,
/// `incurred_on`, `clear_category`, `category`, `clear_receipt_url`,
/// `receipt_url` — exactly the ten values [`ItemPatch`] resolves to.
///
/// `COALESCE($n, column)` is the partial-update rule made mechanical: `NULL`
/// in the binds means "keep". A `NULL` category already means "leave alone",
/// so only the boolean can say "clear". `currency`, `claim_id`, `user_id` and
/// the timestamps are not in the `SET` clause.
const UPDATE_ITEM: &str = r#"
    UPDATE expense_items
    SET description = COALESCE($4, description),
        amount      = COALESCE($5::numeric, amount),
        incurred_on = COALESCE($6::date, incurred_on),
        category    = CASE WHEN $7 THEN NULL ELSE COALESCE($8, category) END,
        receipt_url = CASE WHEN $9 THEN NULL ELSE COALESCE($10, receipt_url) END,
        updated_at  = clock_timestamp()
    WHERE id = $1
      AND claim_id = $2
      AND user_id = $3
    RETURNING id, user_id, claim_id, description,
              amount::text AS amount, incurred_on, category, receipt_url,
              created_at, updated_at
"#;

/// Deletes one item of the caller's claim.
///
/// Both identifiers are bound, so an item id that exists under another claim of
/// the same owner matches no row and becomes the item's `404` — never a `200`
/// and never a `403`.
const DELETE_ITEM: &str =
    "DELETE FROM expense_items WHERE id = $1 AND claim_id = $2 AND user_id = $3";

/// Recomputes a claim's stored total from its items, in the caller's
/// transaction.
///
/// `SUM` over no rows is `NULL`, hence the `COALESCE`: a claim whose last item
/// was deleted stores `0`, which renders `"0.0000"`. The total is never
/// accepted from a client and never computed in Rust (`ADR-0007`).
///
/// The statement is deliberately unconditional about status: it runs only
/// inside the service's transaction, after the claim row was locked and the
/// `draft` guard passed.
const RECOMPUTE_TOTAL: &str = r#"
    UPDATE expense_claims
    SET total_amount = COALESCE(
            (SELECT SUM(amount) FROM expense_items WHERE claim_id = $1 AND user_id = $2),
            0),
        updated_at = clock_timestamp()
    WHERE id = $1 AND user_id = $2
"#;

/// Counts a claim's items, for the submit precondition on the error path.
const COUNT_ITEMS_OWNED: &str =
    "SELECT COUNT(*) FROM expense_items WHERE claim_id = $1 AND user_id = $2";

/// `submit` from `draft` (to `submitted`) or from `rejected` (the reopen).
///
/// One statement covers both edges because the action is ambiguous about its
/// source state, and reading the status first would be a check-then-act: two
/// concurrent submits could both see `draft` and both win. Here the guard is
/// the `WHERE`, and the `CASE` arms are the two rows of the lifecycle table for
/// this action.
///
/// * the **draft** arm sets `submitted_at` and keeps `decided_at`/`paid_at`,
///   which `expense_claims_lifecycle_timestamps_chk` already requires to be
///   null for a draft — and it only applies when the claim has at least one
///   item and a total above zero, which is `REIMB-R05`;
/// * the **reopen** arm returns to `draft`, which requires all three stamps to
///   be null, so `submitted_at` and `decided_at` are cleared in the same
///   statement. Clearing one and not the other would be a `23514` at commit
///   time instead of a clean `200`.
///
/// `clock_timestamp()` rather than `NOW()`: `NOW()` is the transaction's start
/// instant, which for a transaction that waited on the row lock can predate the
/// `submitted_at` it is compared against, and
/// `expense_claims_timestamp_order_chk` would then reject a correct approval.
const TRANSITION_SUBMIT: &str = r#"
    UPDATE expense_claims
    SET status = CASE WHEN status = 'draft'::expense_claim_status
                      THEN 'submitted'::expense_claim_status
                      ELSE 'draft'::expense_claim_status END,
        submitted_at = CASE WHEN status = 'draft'::expense_claim_status
                            THEN clock_timestamp() ELSE NULL END,
        decided_at   = CASE WHEN status = 'draft'::expense_claim_status
                            THEN decided_at ELSE NULL END,
        paid_at      = CASE WHEN status = 'draft'::expense_claim_status
                            THEN paid_at ELSE NULL END,
        updated_at   = clock_timestamp()
    WHERE id = $1
      AND user_id = $2
      AND status IN ('draft'::expense_claim_status, 'rejected'::expense_claim_status)
      AND (status = 'rejected'::expense_claim_status
           OR (total_amount > 0
               AND EXISTS (SELECT 1 FROM expense_items WHERE claim_id = $1 AND user_id = $2)))
    RETURNING id, user_id, title, description, currency,
              total_amount::text AS total_amount, status, project_id,
              submitted_at, decided_at, paid_at, created_at, updated_at
"#;

/// `approve` and `reject`: `submitted` to the bound status, stamping the
/// decision.
///
/// The target is bound rather than written twice, so the two actions share one
/// statement and one guard. `paid_at` stays untouched, and it is null for a
/// submitted claim by the lifecycle constraint.
const TRANSITION_DECIDE: &str = r#"
    UPDATE expense_claims
    SET status     = $3::expense_claim_status,
        decided_at = clock_timestamp(),
        updated_at = clock_timestamp()
    WHERE id = $1
      AND user_id = $2
      AND status = 'submitted'::expense_claim_status
    RETURNING id, user_id, title, description, currency,
              total_amount::text AS total_amount, status, project_id,
              submitted_at, decided_at, paid_at, created_at, updated_at
"#;

/// `pay`: `approved` to `paid`, stamping the payment.
const TRANSITION_PAY: &str = r#"
    UPDATE expense_claims
    SET status     = 'paid'::expense_claim_status,
        paid_at    = clock_timestamp(),
        updated_at = clock_timestamp()
    WHERE id = $1
      AND user_id = $2
      AND status = 'approved'::expense_claim_status
    RETURNING id, user_id, title, description, currency,
              total_amount::text AS total_amount, status, project_id,
              submitted_at, decided_at, paid_at, created_at, updated_at
"#;

/// One new item, as the service validated it.
///
/// A struct rather than six positional arguments: `clippy::too_many_arguments`
/// fires at eight, and an argument order that can be silently swapped is a bug
/// generator. Every value has already been validated and normalised.
#[derive(Debug, Clone, Copy)]
pub struct NewItem<'a> {
    /// The freshly generated identifier.
    pub id: Uuid,
    /// Trimmed description, 1 to 500 characters.
    pub description: &'a str,
    /// Four-decimal money string, never parsed in Rust.
    pub amount: &'a str,
    /// A calendar day, not an instant.
    pub incurred_on: NaiveDate,
    /// Trimmed, lower-cased category, or `None`.
    pub category: Option<&'a str>,
    /// Absolute `http`/`https` reference, or `None`.
    pub receipt_url: Option<&'a str>,
}

/// The resolved tri-state of an item patch, as the statement binds it.
#[derive(Debug, Clone, Copy, Default)]
pub struct ItemPatch<'a> {
    /// New description, or `None` to keep the stored one.
    pub description: Option<&'a str>,
    /// New amount, or `None` to keep the stored one.
    pub amount: Option<&'a str>,
    /// New incurred day, or `None` to keep the stored one.
    pub incurred_on: Option<NaiveDate>,
    /// Whether the category is cleared.
    pub clear_category: bool,
    /// New category, or `None` to keep the stored one.
    pub category: Option<&'a str>,
    /// Whether the receipt reference is cleared.
    pub clear_receipt_url: bool,
    /// New receipt reference, or `None` to keep the stored one.
    pub receipt_url: Option<&'a str>,
}

/// The outcome of one item write.
///
/// The repository never builds an error: the messages, and therefore the
/// statuses, are the service's. Each variant names exactly one thing the caller
/// has to translate, so a new outcome cannot be silently ignored.
#[derive(Debug)]
pub enum ItemWrite<T> {
    /// The write applied and the claim's total was recomputed, in one
    /// transaction, before the commit.
    Applied(T),
    /// No claim of that id belongs to the caller.
    ClaimNotFound,
    /// The claim belongs to the caller but is not a draft, so the write was
    /// refused and rolled back.
    ClaimNotDraft,
    /// The item does not exist under that claim of the caller.
    ItemNotFound,
}

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

    /// Reads one claim of the caller **and** its items from one snapshot.
    ///
    /// Both reads happen inside one `REPEATABLE READ`, read-only transaction,
    /// so the `total_amount` and the items of the returned pair always agree
    /// even while an item write commits. See [`SET_READ_SNAPSHOT`].
    ///
    /// `None` means the claim is absent or another account's — the two cases
    /// are deliberately indistinguishable.
    pub async fn find_detail_owned(
        &self,
        claim_id: Uuid,
        owner: Uuid,
    ) -> AppResult<Option<(ExpenseClaim, Vec<ExpenseItem>)>> {
        let mut transaction = self.pool.begin().await?;

        sqlx::query(SET_READ_SNAPSHOT)
            .execute(&mut *transaction)
            .await?;

        let claim = sqlx::query_as::<_, ExpenseClaim>(FIND_CLAIM_OWNED)
            .bind(claim_id)
            .bind(owner)
            .fetch_optional(&mut *transaction)
            .await?;

        let Some(claim) = claim else {
            // Nothing was written, so the commit only releases the connection.
            transaction.commit().await?;
            return Ok(None);
        };

        let items = sqlx::query_as::<_, ExpenseItem>(LIST_ITEMS_OWNED)
            .bind(claim_id)
            .bind(owner)
            .fetch_all(&mut *transaction)
            .await?;

        transaction.commit().await?;

        Ok(Some((claim, items)))
    }

    /// Counts the items of a claim of the caller.
    ///
    /// Used only on the error path of `submit`: the authoritative precondition
    /// is inside the `UPDATE`, and this count exists to say **which** field the
    /// refusal names.
    pub async fn count_items_owned(&self, claim_id: Uuid, owner: Uuid) -> AppResult<i64> {
        let count = sqlx::query_scalar::<_, i64>(COUNT_ITEMS_OWNED)
            .bind(claim_id)
            .bind(owner)
            .fetch_one(&self.pool)
            .await?;

        Ok(count)
    }

    /// Adds one item to a draft claim and recomputes the total.
    ///
    /// One transaction, in this order:
    ///
    /// 1. lock the claim row and read its status (`404` if absent or foreign,
    ///    `409` if not a draft);
    /// 2. insert the item;
    /// 3. recompute `total_amount` from `SUM(amount)`.
    ///
    /// Steps 2 and 3 therefore cannot diverge: either both commit or neither
    /// does, and the lock keeps a second writer from computing a stale sum in
    /// between.
    pub async fn insert_item(
        &self,
        claim_id: Uuid,
        owner: Uuid,
        item: &NewItem<'_>,
    ) -> AppResult<ItemWrite<ExpenseItem>> {
        let mut transaction = self.pool.begin().await?;

        if let Some(outcome) = lock_draft_claim(&mut transaction, claim_id, owner).await? {
            return Ok(outcome);
        }

        let inserted = sqlx::query_as::<_, ExpenseItem>(INSERT_ITEM)
            .bind(item.id)
            .bind(owner)
            .bind(claim_id)
            .bind(item.description)
            .bind(item.amount)
            .bind(item.incurred_on)
            .bind(item.category)
            .bind(item.receipt_url)
            .fetch_one(&mut *transaction)
            .await?;

        recompute_total(&mut transaction, claim_id, owner).await?;
        transaction.commit().await?;

        Ok(ItemWrite::Applied(inserted))
    }

    /// Applies a partial update to one item of a draft claim and recomputes the
    /// total.
    ///
    /// The same three steps, the same transaction and the same lock as
    /// [`Self::insert_item`]. A zero-row `UPDATE` means the item does not exist
    /// **under that claim** of the caller, which is the item's `404`.
    pub async fn update_item(
        &self,
        claim_id: Uuid,
        owner: Uuid,
        item_id: Uuid,
        patch: &ItemPatch<'_>,
    ) -> AppResult<ItemWrite<ExpenseItem>> {
        let mut transaction = self.pool.begin().await?;

        if let Some(outcome) = lock_draft_claim(&mut transaction, claim_id, owner).await? {
            return Ok(outcome);
        }

        let updated = sqlx::query_as::<_, ExpenseItem>(UPDATE_ITEM)
            .bind(item_id)
            .bind(claim_id)
            .bind(owner)
            .bind(patch.description)
            .bind(patch.amount)
            .bind(patch.incurred_on)
            .bind(patch.clear_category)
            .bind(patch.category)
            .bind(patch.clear_receipt_url)
            .bind(patch.receipt_url)
            .fetch_optional(&mut *transaction)
            .await?;

        let Some(updated) = updated else {
            return Ok(ItemWrite::ItemNotFound);
        };

        recompute_total(&mut transaction, claim_id, owner).await?;
        transaction.commit().await?;

        Ok(ItemWrite::Applied(updated))
    }

    /// Deletes one item of a draft claim and recomputes the total.
    ///
    /// The same transaction and lock as [`Self::insert_item`]. The recompute
    /// turns `SUM` over no rows into `0`, so a claim whose last item was
    /// removed stores `"0.0000"` rather than a stale total.
    pub async fn delete_item(
        &self,
        claim_id: Uuid,
        owner: Uuid,
        item_id: Uuid,
    ) -> AppResult<ItemWrite<()>> {
        let mut transaction = self.pool.begin().await?;

        if let Some(outcome) = lock_draft_claim(&mut transaction, claim_id, owner).await? {
            return Ok(outcome);
        }

        let deleted = sqlx::query(DELETE_ITEM)
            .bind(item_id)
            .bind(claim_id)
            .bind(owner)
            .execute(&mut *transaction)
            .await?;

        if deleted.rows_affected() == 0 {
            return Ok(ItemWrite::ItemNotFound);
        }

        recompute_total(&mut transaction, claim_id, owner).await?;
        transaction.commit().await?;

        Ok(ItemWrite::Applied(()))
    }

    /// Applies one lifecycle action to one claim of the caller.
    ///
    /// One `UPDATE` per action, with the expected source status in the `WHERE`,
    /// so two concurrent actions cannot both win: the second re-evaluates the
    /// guard against the committed row and touches nothing. Zero rows is
    /// therefore ambiguous — absent or foreign, wrong status, or a refused
    /// submit precondition — and the service resolves it [^error-path] on its
    /// error path with one owner-scoped read.
    ///
    /// [^error-path]: The read is owner-scoped, changes no state, and happens
    /// only after the write has already refused; the decision itself is the
    /// statement.
    pub async fn transition_owned(
        &self,
        claim_id: Uuid,
        owner: Uuid,
        action: LifecycleAction,
    ) -> AppResult<Option<ExpenseClaim>> {
        let claim = match action {
            LifecycleAction::Submit => {
                sqlx::query_as::<_, ExpenseClaim>(TRANSITION_SUBMIT)
                    .bind(claim_id)
                    .bind(owner)
                    .fetch_optional(&self.pool)
                    .await?
            }
            LifecycleAction::Approve | LifecycleAction::Reject => {
                let target = if action == LifecycleAction::Approve {
                    ClaimStatus::Approved
                } else {
                    ClaimStatus::Rejected
                };

                sqlx::query_as::<_, ExpenseClaim>(TRANSITION_DECIDE)
                    .bind(claim_id)
                    .bind(owner)
                    .bind(target)
                    .fetch_optional(&self.pool)
                    .await?
            }
            LifecycleAction::Pay => {
                sqlx::query_as::<_, ExpenseClaim>(TRANSITION_PAY)
                    .bind(claim_id)
                    .bind(owner)
                    .fetch_optional(&self.pool)
                    .await?
            }
        };

        Ok(claim)
    }
}

/// Locks a claim of the caller and refuses it when it is not a draft.
///
/// `Ok(None)` means the claim is the caller's and a draft, so the caller may
/// write. `Ok(Some(outcome))` is the refusal the caller returns as it is.
async fn lock_draft_claim<T>(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    claim_id: Uuid,
    owner: Uuid,
) -> AppResult<Option<ItemWrite<T>>> {
    let status = sqlx::query_scalar::<_, ClaimStatus>(LOCK_CLAIM)
        .bind(claim_id)
        .bind(owner)
        .fetch_optional(&mut **transaction)
        .await?;

    match status {
        None => Ok(Some(ItemWrite::ClaimNotFound)),
        Some(ClaimStatus::Draft) => Ok(None),
        Some(_) => Ok(Some(ItemWrite::ClaimNotDraft)),
    }
}

/// Recomputes a claim's total inside the caller's transaction.
async fn recompute_total(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    claim_id: Uuid,
    owner: Uuid,
) -> AppResult<()> {
    sqlx::query(RECOMPUTE_TOTAL)
        .bind(claim_id)
        .bind(owner)
        .execute(&mut **transaction)
        .await?;

    Ok(())
}
