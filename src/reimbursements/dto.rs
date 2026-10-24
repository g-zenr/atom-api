//! Public contracts for the `reimbursements` module.
//!
//! The shapes mirror [`docs/features/reimbursements/api.md`] §2.2–§2.5 field
//! for field. Two properties are carried by the types themselves rather than
//! by a convention a caller has to remember:
//!
//! * **`user_id` never reaches the wire.** The response types are built from
//!   the row models and drop it, exactly as `ProjectResponse` drops its own.
//! * **Money is a `String` with four decimals.** `total_amount` and `amount`
//!   are `NUMERIC(19,4)::text` on the server side and are never parsed,
//!   formatted or arithmetically combined in Rust (ADR-0007). A JSON number is
//!   therefore impossible to produce from these types, and the OpenAPI
//!   components say `type: string` with the four-decimal pattern.
//!
//! The two request bodies are deliberately narrow. `currency`, `status`,
//! `total_amount`, `id`, `created_at` and the lifecycle timestamps are **not**
//! fields of [`UpdateClaimRequest`], so a body carrying them is accepted and
//! ignored: the column is absent from the `SET` clause of every statement,
//! which is a stronger guarantee than a rejection would be.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::pagination::{Page, Pagination, PaginationQuery};
use crate::reimbursements::models::{ClaimStatus, ExpenseClaim, ExpenseItem};

/// Maximum accepted claim title length.
///
/// Mirrors the database constraint `expense_claims_title_length_chk`
/// (`CHECK (CHAR_LENGTH(title) BETWEEN 1 AND 200)`) in
/// `migrations/0004_expense_claims.sql`. The migration is the enforced copy;
/// this constant is the application-side mirror of it.
pub const MAX_TITLE_LENGTH: usize = 200;

/// Maximum accepted claim description length.
///
/// Mirrors the database constraint `expense_claims_description_length_chk`
/// (`CHECK (CHAR_LENGTH(description) <= 5000)`) in
/// `migrations/0004_expense_claims.sql`. The migration is the enforced copy;
/// this constant is the application-side mirror of it.
pub const MAX_DESCRIPTION_LENGTH: usize = 5_000;

/// The public representation of a claim, as the listing returns it.
///
/// The listing carries no items: [`ClaimDetailResponse`] is the shape that
/// does. `user_id` is on the row and is deliberately absent here.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ClaimResponse {
    /// Stable public identifier.
    pub id: Uuid,
    /// Claim title, 1 to 200 characters.
    pub title: String,
    /// Claim description; may be empty.
    pub description: String,
    /// ISO 4217 code, three upper-case letters. Immutable once created.
    pub currency: String,
    /// The sum of the claim's items, as a four-decimal string.
    ///
    /// Server-owned: no request body carries it.
    #[schema(
        format = "decimal",
        pattern = "^[0-9]{1,15}\\.[0-9]{4}$",
        example = "149.8000"
    )]
    pub total_amount: String,
    /// Current lifecycle state.
    pub status: ClaimStatus,
    /// The optional project link.
    pub project_id: Option<Uuid>,
    /// Set exactly when the status requires it.
    pub submitted_at: Option<DateTime<Utc>>,
    /// The approve or reject instant.
    pub decided_at: Option<DateTime<Utc>>,
    /// The payment instant.
    pub paid_at: Option<DateTime<Utc>>,
    /// Creation timestamp, RFC 3339 UTC.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp, RFC 3339 UTC.
    pub updated_at: DateTime<Utc>,
}

/// Builds the public representation of a claim.
///
/// The model is consumed rather than borrowed, so that the strings of a
/// listed page are moved into the response instead of being copied.
impl From<ExpenseClaim> for ClaimResponse {
    fn from(claim: ExpenseClaim) -> Self {
        Self {
            id: claim.id,
            title: claim.title,
            description: claim.description,
            currency: claim.currency,
            total_amount: claim.total_amount,
            status: claim.status,
            project_id: claim.project_id,
            submitted_at: claim.submitted_at,
            decided_at: claim.decided_at,
            paid_at: claim.paid_at,
            created_at: claim.created_at,
            updated_at: claim.updated_at,
        }
    }
}

/// The public representation of a claim line item.
///
/// Published with `R2` because the claim detail response contains items; the
/// endpoints that write an item are `R3`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ItemResponse {
    /// Stable public identifier, addressable only inside its claim.
    pub id: Uuid,
    /// The claim this item belongs to.
    pub claim_id: Uuid,
    /// Line description, 1 to 500 characters.
    pub description: String,
    /// The amount, as a four-decimal string in the claim's currency.
    #[schema(
        format = "decimal",
        pattern = "^[0-9]{1,15}\\.[0-9]{4}$",
        example = "125.8000"
    )]
    pub amount: String,
    /// The day the expense was incurred, `YYYY-MM-DD`.
    pub incurred_on: NaiveDate,
    /// Optional lower-case category token.
    pub category: Option<String>,
    /// Optional client-supplied `http` or `https` reference.
    pub receipt_url: Option<String>,
    /// Creation timestamp, RFC 3339 UTC.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp, RFC 3339 UTC.
    pub updated_at: DateTime<Utc>,
}

/// Builds the public representation of an item. `user_id` is dropped.
impl From<ExpenseItem> for ItemResponse {
    fn from(item: ExpenseItem) -> Self {
        Self {
            id: item.id,
            claim_id: item.claim_id,
            description: item.description,
            amount: item.amount,
            incurred_on: item.incurred_on,
            category: item.category,
            receipt_url: item.receipt_url,
            created_at: item.created_at,
            updated_at: item.updated_at,
        }
    }
}

/// The public representation of one claim, with its items.
///
/// The shape is **flat** — the twelve [`ClaimResponse`] fields and `items` side
/// by side — because that is the published JSON. It is not `{ claim, items }`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ClaimDetailResponse {
    /// Stable public identifier.
    pub id: Uuid,
    /// Claim title, 1 to 200 characters.
    pub title: String,
    /// Claim description; may be empty.
    pub description: String,
    /// ISO 4217 code, three upper-case letters. Immutable once created.
    pub currency: String,
    /// The sum of the claim's items, as a four-decimal string.
    #[schema(
        format = "decimal",
        pattern = "^[0-9]{1,15}\\.[0-9]{4}$",
        example = "149.8000"
    )]
    pub total_amount: String,
    /// Current lifecycle state.
    pub status: ClaimStatus,
    /// The optional project link.
    pub project_id: Option<Uuid>,
    /// Set exactly when the status requires it.
    pub submitted_at: Option<DateTime<Utc>>,
    /// The approve or reject instant.
    pub decided_at: Option<DateTime<Utc>>,
    /// The payment instant.
    pub paid_at: Option<DateTime<Utc>>,
    /// Creation timestamp, RFC 3339 UTC.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp, RFC 3339 UTC.
    pub updated_at: DateTime<Utc>,
    /// The claim's line items, oldest incurred day first.
    pub items: Vec<ItemResponse>,
}

impl ClaimDetailResponse {
    /// Builds the detail response from a claim and its items.
    pub fn new(claim: ExpenseClaim, items: Vec<ExpenseItem>) -> Self {
        let ClaimResponse {
            id,
            title,
            description,
            currency,
            total_amount,
            status,
            project_id,
            submitted_at,
            decided_at,
            paid_at,
            created_at,
            updated_at,
        } = ClaimResponse::from(claim);

        Self {
            id,
            title,
            description,
            currency,
            total_amount,
            status,
            project_id,
            submitted_at,
            decided_at,
            paid_at,
            created_at,
            updated_at,
            items: items.into_iter().map(ItemResponse::from).collect(),
        }
    }
}

/// The response body of `GET /api/v1/claims`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ClaimPage {
    /// The claims on this page.
    pub items: Vec<ClaimResponse>,
    /// Pagination metadata.
    pub pagination: Pagination,
}

impl From<Page<ClaimResponse>> for ClaimPage {
    fn from(page: Page<ClaimResponse>) -> Self {
        Self {
            items: page.items,
            pagination: page.pagination,
        }
    }
}

/// Request body for creating a claim.
///
/// A claim is always created in `draft`, with a total of `"0.0000"` and no
/// items. `status`, `total_amount` and the lifecycle timestamps are
/// server-owned and are not part of this request.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateClaimRequest {
    /// Claim title, trimmed to 1 to 200 characters.
    #[schema(example = "Client visit to Berlin")]
    pub title: String,
    /// Claim description. Defaults to an empty string.
    #[schema(example = "Two days on site")]
    #[serde(default)]
    pub description: String,
    /// ISO 4217 code. Trimmed and upper-cased; exactly three letters.
    #[schema(example = "EUR")]
    pub currency: String,
    /// The optional project link. It must belong to the caller.
    pub project_id: Option<Uuid>,
}

/// Request body for partially updating a claim.
///
/// Every field is optional. Omitted fields are left untouched; they are never
/// written as `null`.
///
/// `currency`, `status`, `total_amount`, `id`, `created_at` and the three
/// lifecycle timestamps are **not** fields here. serde ignores unknown keys by
/// default, so a body carrying one of them is accepted and it changes nothing.
/// `#[serde(deny_unknown_fields)]` must not be added: it would turn every
/// unknown key into a `422`, which is a breaking change for a client that
/// sends one extra field.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateClaimRequest {
    /// New title. Trimmed; an empty title is a validation failure, not a clear.
    #[schema(example = "Client visit to Berlin (Oct)")]
    pub title: Option<String>,
    /// New description. `""` clears it; an absent field leaves it untouched.
    pub description: Option<String>,
    /// The project link, as a tri-state.
    ///
    /// * key absent — [`None`]: leave the link untouched;
    /// * `"project_id": null` — `Some(None)`: clear the link;
    /// * `"project_id": "<uuid>"` — `Some(Some(id))`: set it, if it belongs to
    ///   the caller.
    ///
    /// `Option<Option<T>>` alone is not enough: `serde_json` answers an
    /// explicit `null` with `visit_none` on the *outer* `Option`, so `{}` and
    /// `{"project_id": null}` would both deserialize as `None`. The pair of
    /// attributes below is what keeps the three states apart, and
    /// [`double_option`] documents what each one does.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<Uuid>, nullable)]
    pub project_id: Option<Option<Uuid>>,
}

impl UpdateClaimRequest {
    /// The two parameters the update statement binds: `(clear, value)`.
    ///
    /// A single `COALESCE($n, project_id)` cannot express "set to `NULL`",
    /// because `NULL` already means "leave alone", which is why the boolean
    /// exists.
    pub fn project_change(&self) -> (bool, Option<Uuid>) {
        match self.project_id {
            None => (false, None),
            Some(None) => (true, None),
            Some(Some(id)) => (false, Some(id)),
        }
    }
}

/// Distinguishes `"project_id": null` from an absent key.
///
/// `#[serde(default)]` handles absence (`None`) and short-circuits this
/// function; when the key *is* present the function runs, and `null`
/// deserializes as `Option::<Uuid>::deserialize(..) == None`, wrapped into
/// `Some(None)`. Drop `default` and the missing-field deserializer answers
/// `visit_none`, which puts absence back to `Some(None)`; drop this function
/// and `null` becomes `None`. Both are required.
fn double_option<'de, D>(deserializer: D) -> Result<Option<Option<Uuid>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<Uuid>::deserialize(deserializer).map(Some)
}

/// Query parameters accepted by `GET /api/v1/claims`.
///
/// Only the feature filters live here. The pagination parameters live in
/// [`PaginationQuery`], and the handler reads both halves from a single
/// [`ClaimsListQuery`]: declaring `limit` and `offset` in both places would
/// publish them twice in the generated OpenAPI document.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ClaimQuery {
    /// Filter by lifecycle status. An unknown value is a `400`.
    pub status: Option<ClaimStatus>,
    /// Filter by currency. Trimmed and upper-cased; exactly three letters.
    #[param(example = "EUR")]
    pub currency: Option<String>,
    /// Case-insensitive substring over title and description.
    ///
    /// At most 200 characters. An empty term means "no filter"; a longer one
    /// is a validation failure.
    #[param(example = "berlin")]
    pub search: Option<String>,
    /// Inclusive lower bound on `submitted_at`, `YYYY-MM-DD`, UTC.
    #[param(example = "2026-10-01")]
    pub submitted_from: Option<NaiveDate>,
    /// Inclusive upper bound on `submitted_at`, `YYYY-MM-DD`, UTC.
    #[param(example = "2026-10-31")]
    pub submitted_to: Option<NaiveDate>,
}

/// The whole query string of `GET /api/v1/claims`, deserialized once.
///
/// The handler takes a single `Query<…>` extractor over this type. The query
/// string is therefore parsed exactly once, and [`Self::into_parts`] splits the
/// result into the pair the service layer works with.
///
/// The fields are written out instead of composing [`ClaimQuery`] and
/// [`PaginationQuery`] with `#[serde(flatten)]`. The extractor deserializes
/// with `serde_urlencoded`, whose map access yields strings, so a flattened
/// `Option<i64>` would be asked to deserialize itself from a string and would
/// fail at run time, on a real request, instead of at compile time. The
/// duplication is the price of parsing the string once.
///
/// It derives `Deserialize` only, and is deliberately absent from the
/// generated OpenAPI document: the published parameter set stays [`ClaimQuery`]
/// and [`PaginationQuery`], which the handler's `utoipa` attribute names.
#[derive(Debug, Clone, Deserialize)]
pub struct ClaimsListQuery {
    /// Maximum number of claims to return.
    pub limit: Option<i64>,
    /// Number of claims to skip.
    pub offset: Option<i64>,
    /// Filter by lifecycle status.
    pub status: Option<ClaimStatus>,
    /// Filter by currency.
    pub currency: Option<String>,
    /// Free-text search over title and description.
    pub search: Option<String>,
    /// Inclusive lower bound on `submitted_at`.
    pub submitted_from: Option<NaiveDate>,
    /// Inclusive upper bound on `submitted_at`.
    pub submitted_to: Option<NaiveDate>,
}

impl ClaimsListQuery {
    /// Splits the parsed query into its filter and pagination halves.
    pub fn into_parts(self) -> (ClaimQuery, PaginationQuery) {
        let Self {
            limit,
            offset,
            status,
            currency,
            search,
            submitted_from,
            submitted_to,
        } = self;

        (
            ClaimQuery {
                status,
                currency,
                search,
                submitted_from,
                submitted_to,
            },
            PaginationQuery { limit, offset },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `#[serde(default)]` and `double_option` pair of the design
    /// document §5.2. Drop either one and this test fails, because absence and
    /// an explicit `null` collapse into the same value.
    #[test]
    fn a_project_link_is_kept_cleared_or_set_by_the_three_body_forms() {
        let kept: UpdateClaimRequest =
            serde_json::from_str("{}").expect("an empty body is a valid patch");
        assert_eq!(kept.project_change(), (false, None));

        let cleared: UpdateClaimRequest = serde_json::from_str(r#"{"project_id": null}"#)
            .expect("an explicit null is a valid patch");
        assert_eq!(cleared.project_change(), (true, None));

        let id = Uuid::new_v4();
        let set: UpdateClaimRequest = serde_json::from_str(&format!(r#"{{"project_id":"{id}"}}"#))
            .expect("a UUID is a valid patch");
        assert_eq!(set.project_change(), (false, Some(id)));
    }
}
