//! Claim and item use cases: validation, the lifecycle table and the
//! ownership mapping.
//!
//! Every method takes `owner` first, exactly as `ProjectService` does, so a
//! claim operation cannot be called without saying whose claim is meant. The
//! service never compares identifiers and never returns `403`: a repository
//! miss becomes [`AppError::NotFound`] with the same message for "does not
//! exist" and "belongs to somebody else" (`ADR-0005` §3).
//!
//! Three rules hold in this order:
//!
//! 1. **Validate the whole body first, then touch the database.** A request
//!    that is both malformed and aimed at a foreign resource is a `422`, not a
//!    `404`: the body never had to be read.
//! 2. **Every miss is a catalogue message**, never a message assembled here.
//! 3. **Every state change is guarded by the statement, not by a preceding
//!    read.** The claim row lock and the `status = …` predicate live in the
//!    SQL, so two concurrent writers cannot both pass a check and then both
//!    write.
//!
//! Money is validated here but never parsed: [`validate_money`] is a character
//! check that returns the string unchanged, and the only arithmetic is the
//! `SUM` in SQL (`ADR-0007`).

use std::sync::Arc;

use chrono::NaiveDate;
use uuid::Uuid;

use crate::config::messages::{Messages, render};
use crate::config::settings::PaginationSettings;
use crate::error::{AppError, AppResult};
use crate::helpers::validation::validate_money;
use crate::helpers::{bounded_text, field_error, required_text, validate_search};
use crate::pagination::{Page, PaginationQuery};
use crate::reimbursements::dto::{
    ClaimDetailResponse, ClaimQuery, ClaimResponse, CreateClaimRequest, CreateItemRequest,
    ItemResponse, MAX_DESCRIPTION_LENGTH, MAX_ITEM_CATEGORY_LENGTH, MAX_ITEM_DESCRIPTION_LENGTH,
    MAX_RECEIPT_URL_LENGTH, MAX_TITLE_LENGTH, UpdateClaimRequest, UpdateItemRequest,
};
use crate::reimbursements::models::ClaimStatus;
use crate::reimbursements::repository::{
    ClaimListFilter, ClaimRepository, ItemPatch, ItemWrite, NewItem,
};
use crate::utils::text::char_len;

/// One of the four published lifecycle actions.
///
/// It is not a request field: the action is the route, so no body can choose
/// it. It exists as a type so that the transition table has exactly one input
/// and the four handlers cannot each spell the rule differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleAction {
    /// `POST …/submit` — submit a draft, or reopen a rejected claim.
    Submit,
    /// `POST …/approve` — approve a submitted claim.
    Approve,
    /// `POST …/reject` — reject a submitted claim.
    Reject,
    /// `POST …/pay` — pay an approved claim.
    Pay,
}

/// The transition table, and the only implementation of it.
///
/// | from | action | to |
/// | ---- | ------ | -- |
/// | `draft` | `submit` | `submitted` |
/// | `submitted` | `approve` | `approved` |
/// | `submitted` | `reject` | `rejected` |
/// | `approved` | `pay` | `paid` |
/// | `rejected` | `submit` | `draft` (the reopen) |
///
/// Every other pair of the twenty combinations is refused, and `paid` is
/// terminal. `None` means "not in the table", which the service reports as
/// `409 conflict`.
///
/// The success path does **not** call this function to decide whether to write:
/// the guard is the `WHERE status = …` of the `UPDATE`, so a caller cannot be
/// between the check and the write. This function is the single definition of
/// the table that the refusal explains, and of the edge each action implements;
/// the unit test below is the exhaustive proof of it.
pub fn ensure_transition(current: ClaimStatus, action: LifecycleAction) -> Option<ClaimStatus> {
    use ClaimStatus::{Approved, Draft, Paid, Rejected, Submitted};

    match (current, action) {
        (Draft, LifecycleAction::Submit) => Some(Submitted),
        (Submitted, LifecycleAction::Approve) => Some(Approved),
        (Submitted, LifecycleAction::Reject) => Some(Rejected),
        (Approved, LifecycleAction::Pay) => Some(Paid),
        (Rejected, LifecycleAction::Submit) => Some(Draft),
        _ => None,
    }
}

/// Claim use cases.
#[derive(Debug, Clone)]
pub struct ClaimService {
    repository: ClaimRepository,
    messages: Arc<Messages>,
    pagination: PaginationSettings,
}

impl ClaimService {
    /// Builds the service.
    pub fn new(
        repository: ClaimRepository,
        messages: Arc<Messages>,
        pagination: PaginationSettings,
    ) -> Self {
        Self {
            repository,
            messages,
            pagination,
        }
    }

    /// Lists the owner's claims, newest first, without items.
    pub async fn list(
        &self,
        owner: Uuid,
        query: ClaimQuery,
        pagination: PaginationQuery,
    ) -> AppResult<Page<ClaimResponse>> {
        // Validate every filter before the statement runs, and in the order the
        // published parameter table lists them.
        let currency = query
            .currency
            .as_deref()
            .map(|value| validate_currency(value, &self.messages))
            .transpose()?;
        let search = query
            .search
            .as_deref()
            .map(|term| normalize_search(term, &self.messages))
            .transpose()?
            .flatten();
        validate_submission_range(query.submitted_from, query.submitted_to, &self.messages)?;

        let filter = ClaimListFilter {
            status: query.status,
            currency: currency.as_deref(),
            search: search.as_deref(),
            submitted_from: query.submitted_from,
            submitted_to: query.submitted_to,
        };

        let limit = pagination.effective_limit(&self.pagination);
        let offset = pagination.effective_offset();

        let (claims, total) = self
            .repository
            .list_owned(owner, &filter, limit, offset)
            .await?;

        // The page is consumed: the strings of every claim are moved into the
        // response rather than copied.
        let items = claims.into_iter().map(ClaimResponse::from).collect();

        Ok(Page::new(items, limit, offset, total))
    }

    /// Creates a claim in `draft` for the given owner.
    ///
    /// A missing or foreign `project_id` is the project's `404`; the ownership
    /// predicate runs inside the insert statement, so there is no second round
    /// trip on the success path and no gap between the check and the write.
    pub async fn create(
        &self,
        owner: Uuid,
        request: CreateClaimRequest,
    ) -> AppResult<ClaimDetailResponse> {
        let title = validate_title(&request.title, &self.messages)?;
        let description = validate_description(&request.description, &self.messages)?;
        let currency = validate_currency(&request.currency, &self.messages)?;

        let claim = self
            .repository
            .insert(
                Uuid::new_v4(),
                owner,
                &title,
                &description,
                &currency,
                request.project_id,
            )
            .await?
            .ok_or_else(|| AppError::NotFound(self.messages.resources.project_not_found.clone()))?;

        // A claim that was just created cannot have items: the item table
        // references the claim, and no request can add an item until `R3`.
        Ok(ClaimDetailResponse::new(claim, Vec::new()))
    }

    /// Loads one of the owner's claims with its items.
    ///
    /// The claim and its items come from **one** snapshot, so the returned
    /// `total_amount` always equals the sum of the returned items even while an
    /// item write commits (`docs/architecture/reimbursements-crud-design.md`
    /// §6.7, decided here in favour of the consistent pair).
    pub async fn get(&self, owner: Uuid, claim_id: Uuid) -> AppResult<ClaimDetailResponse> {
        let (claim, items) = self
            .repository
            .find_detail_owned(claim_id, owner)
            .await?
            .ok_or_else(|| AppError::NotFound(self.messages.resources.claim_not_found.clone()))?;

        Ok(ClaimDetailResponse::new(claim, items))
    }

    /// Applies a partial update to one of the owner's claims.
    ///
    /// The write is one statement. When it affects no row, one owner-scoped
    /// read on the **error path** says which resource was the problem: if the
    /// claim exists, the project was foreign or absent; otherwise the claim
    /// was.
    pub async fn update(
        &self,
        owner: Uuid,
        claim_id: Uuid,
        request: UpdateClaimRequest,
    ) -> AppResult<ClaimDetailResponse> {
        let title = request
            .title
            .as_deref()
            .map(|title| validate_title(title, &self.messages))
            .transpose()?;
        let description = request
            .description
            .as_deref()
            .map(|description| validate_description(description, &self.messages))
            .transpose()?;
        let (clear_project, project_id) = request.project_change();

        let claim = self
            .repository
            .update_owned(
                claim_id,
                owner,
                title.as_deref(),
                description.as_deref(),
                clear_project,
                project_id,
            )
            .await?;

        let Some(claim) = claim else {
            // Zero rows: either the claim is absent or foreign, or the project
            // is. The write already refused; this read only names the resource
            // and changes no state.
            let exists = self.repository.find_owned(claim_id, owner).await?.is_some();

            return Err(AppError::NotFound(if exists {
                self.messages.resources.project_not_found.clone()
            } else {
                self.messages.resources.claim_not_found.clone()
            }));
        };

        self.get(owner, claim.id).await
    }

    /// Deletes one of the owner's claims. Its items go with it.
    pub async fn delete(&self, owner: Uuid, claim_id: Uuid) -> AppResult<()> {
        let deleted = self.repository.delete_owned(claim_id, owner).await?;

        if !deleted {
            return Err(AppError::NotFound(
                self.messages.resources.claim_not_found.clone(),
            ));
        }

        Ok(())
    }

    /// Adds one item to a draft claim and returns it.
    ///
    /// The claim's new total is not part of this response — the published
    /// contract says the `ItemResponse` is the whole body, and the total is read
    /// with the claim.
    ///
    /// An amount that would push the claim's total past what `NUMERIC(19,4)`
    /// can store is a `422` naming `amount`: the repository's guard refuses the
    /// total and rolls the whole write back, so nothing is inserted and the
    /// stored total is unchanged. Each item may carry fifteen integer digits
    /// on its own, so this is reachable with two legal values.
    pub async fn create_item(
        &self,
        owner: Uuid,
        claim_id: Uuid,
        request: CreateItemRequest,
    ) -> AppResult<ItemResponse> {
        let description = validate_item_description(&request.description, &self.messages)?;
        let amount = validate_money("amount", &request.amount, &self.messages)?;
        let category = request
            .category
            .as_deref()
            .map(|category| normalize_category(category, &self.messages))
            .transpose()?
            .flatten();
        let receipt_url = request
            .receipt_url
            .as_deref()
            .map(|url| validate_receipt_url(url, &self.messages))
            .transpose()?;

        let item = self
            .repository
            .insert_item(
                claim_id,
                owner,
                &NewItem {
                    id: Uuid::new_v4(),
                    description: &description,
                    amount: &amount,
                    incurred_on: request.incurred_on,
                    category: category.as_deref(),
                    receipt_url: receipt_url.as_deref(),
                },
            )
            .await?;

        Ok(ItemResponse::from(self.item_result(item)?))
    }

    /// Applies a partial update to one item of a draft claim.
    ///
    /// Every field is optional. `category` and `receipt_url` are tri-states: an
    /// omitted key leaves the column untouched, an explicit `null` or an empty
    /// category clears it.
    ///
    /// A new amount that would push the claim's total past `NUMERIC(19,4)` is a
    /// `422` naming `amount`, and the patch is rolled back as a whole: the item
    /// keeps its previous amount and the claim keeps its previous total.
    pub async fn update_item(
        &self,
        owner: Uuid,
        claim_id: Uuid,
        item_id: Uuid,
        request: UpdateItemRequest,
    ) -> AppResult<ItemResponse> {
        let description = request
            .description
            .as_deref()
            .map(|description| validate_item_description(description, &self.messages))
            .transpose()?;
        let amount = request
            .amount
            .as_deref()
            .map(|amount| validate_money("amount", amount, &self.messages))
            .transpose()?;
        let (clear_category, category) =
            resolve_category(request.category_change(), &self.messages)?;
        let (clear_receipt_url, receipt_url) =
            resolve_receipt_url(request.receipt_url_change(), &self.messages)?;

        let item = self
            .repository
            .update_item(
                claim_id,
                owner,
                item_id,
                &ItemPatch {
                    description: description.as_deref(),
                    amount: amount.as_deref(),
                    incurred_on: request.incurred_on,
                    clear_category,
                    category: category.as_deref(),
                    clear_receipt_url,
                    receipt_url: receipt_url.as_deref(),
                },
            )
            .await?;

        Ok(ItemResponse::from(self.item_result(item)?))
    }

    /// Deletes one item of a draft claim.
    ///
    /// The item's identifier alone is never enough to address a row: the
    /// statement also binds the claim and the owner, so an item that exists
    /// under a different claim of the same caller is a `404`.
    pub async fn delete_item(&self, owner: Uuid, claim_id: Uuid, item_id: Uuid) -> AppResult<()> {
        let outcome = self
            .repository
            .delete_item(claim_id, owner, item_id)
            .await?;

        self.item_result(outcome)?;

        Ok(())
    }

    /// Applies one lifecycle action to one of the owner's claims.
    ///
    /// The transition itself is one guarded `UPDATE`. A zero-row result is
    /// explained on the error path by [`Self::explain_refusal`], which reads the
    /// claim once — after the write has already refused — and turns the state
    /// into the status the contract publishes: `404`, `409`, or the `422` of a
    /// refused submit precondition.
    pub async fn transition(
        &self,
        owner: Uuid,
        claim_id: Uuid,
        action: LifecycleAction,
    ) -> AppResult<ClaimDetailResponse> {
        if self
            .repository
            .transition_owned(claim_id, owner, action)
            .await?
            .is_none()
        {
            return Err(self.explain_refusal(owner, claim_id, action).await?);
        }

        self.get(owner, claim_id).await
    }

    /// Explains why a guarded transition affected no row.
    ///
    /// The guarded statement is the decision; this read only names its reason,
    /// so the order matters:
    ///
    /// 1. the claim is absent or foreign → `404`;
    /// 2. the transition table does not allow the pair → `409`;
    /// 3. the table allows it but the statement still refused, which for
    ///    `submit` from `draft` is the "at least one item, total above zero"
    ///    rule → `422` naming `items`, or `total_amount` when the claim has
    ///    items but a zero total;
    /// 4. otherwise a concurrent action moved the claim between the statement
    ///    and this read → `409`.
    async fn explain_refusal(
        &self,
        owner: Uuid,
        claim_id: Uuid,
        action: LifecycleAction,
    ) -> AppResult<AppError> {
        let Some(claim) = self.repository.find_owned(claim_id, owner).await? else {
            return Ok(AppError::NotFound(
                self.messages.resources.claim_not_found.clone(),
            ));
        };

        if ensure_transition(claim.status, action).is_none() {
            return Ok(AppError::Conflict(
                self.messages.conflicts.transition_not_allowed.clone(),
            ));
        }

        if action == LifecycleAction::Submit && claim.status == ClaimStatus::Draft {
            let items = self.repository.count_items_owned(claim_id, owner).await?;

            return Ok(if items == 0 {
                field_error(
                    "items",
                    self.messages.validation.items_required.clone(),
                    &self.messages,
                )
            } else {
                field_error(
                    "total_amount",
                    self.messages.validation.total_positive.clone(),
                    &self.messages,
                )
            });
        }

        Ok(AppError::Conflict(
            self.messages.conflicts.transition_not_allowed.clone(),
        ))
    }

    /// Translates one item-write outcome, using only catalogue messages.
    fn item_result<T>(&self, outcome: ItemWrite<T>) -> AppResult<T> {
        item_refusal(outcome, &self.messages)
    }
}

/// Translates one item-write outcome into the response the contract publishes.
///
/// A free function rather than a method: the translation reads nothing but the
/// catalogue, so the unit test can prove it without a pool and without a Tokio
/// runtime. The service's [`ClaimService::item_result`] is the only caller.
fn item_refusal<T>(outcome: ItemWrite<T>, messages: &Messages) -> AppResult<T> {
    match outcome {
        ItemWrite::Applied(value) => Ok(value),
        ItemWrite::ClaimNotFound => Err(AppError::NotFound(
            messages.resources.claim_not_found.clone(),
        )),
        ItemWrite::ClaimNotDraft => Err(AppError::Conflict(
            messages.conflicts.claim_not_editable.clone(),
        )),
        // The repository rolled the write back, so the claim still holds its
        // previous items and total. The request was well formed and the field
        // is the one to change, which is a `422` naming `amount` rather than
        // the column's `22003`.
        ItemWrite::TotalExceedsMaximum => Err(field_error(
            "amount",
            messages.validation.claim_total_exceeds_maximum.clone(),
            messages,
        )),
        ItemWrite::ItemNotFound => Err(AppError::NotFound(
            messages.resources.item_not_found.clone(),
        )),
    }
}

/// Validates a claim title.
///
/// The title is trimmed before it is measured, so 200 characters surrounded by
/// spaces is accepted and stored trimmed. A title that is empty after trimming
/// is rejected: a claim always has a title, so an empty one is not a "clear".
pub fn validate_title(title: &str, messages: &Messages) -> AppResult<String> {
    required_text(
        "title",
        title,
        MAX_TITLE_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

/// Validates a claim description.
///
/// The description is kept verbatim, so `""` clears it and a description of
/// spaces is stored as it was sent. That mirrors the column, which is
/// `NOT NULL DEFAULT ''` and measured by `expense_claims_description_length_chk`.
pub fn validate_description(description: &str, messages: &Messages) -> AppResult<String> {
    bounded_text(
        "description",
        description,
        MAX_DESCRIPTION_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

/// Validates and normalises a currency code.
///
/// The value is trimmed and upper-cased, then it must be exactly three ASCII
/// letters. The database column is `CHAR(3)` with
/// `expense_claims_currency_chk (currency ~ '^[A-Z]{3}$')`, so the written value
/// is always upper case; this check makes a lower-case client input a
/// normalisation instead of a constraint violation, and an invalid one a `422`
/// before any statement runs.
pub fn validate_currency(currency: &str, messages: &Messages) -> AppResult<String> {
    let normalised = currency.trim().to_ascii_uppercase();

    if normalised.len() != 3 || !normalised.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(field_error(
            "currency",
            messages.validation.currency_invalid.clone(),
            messages,
        ));
    }

    Ok(normalised)
}

/// Normalises a search term: trimmed, and `None` when it is empty.
///
/// An empty term means "no filter", which is the published rule for this
/// parameter (and for no other). It is not an error.
pub fn normalize_search(term: &str, messages: &Messages) -> AppResult<Option<String>> {
    let trimmed = validate_search(term, messages)?;

    Ok(Some(trimmed).filter(|value| !value.is_empty()))
}

/// Validates an item description.
///
/// Trimmed before it is measured, so 500 characters surrounded by spaces is
/// accepted and stored trimmed, and an empty description is a `422`: the column
/// is `CHECK (CHAR_LENGTH(description) BETWEEN 1 AND 500)`, so it has no "clear"
/// spelling.
pub fn validate_item_description(description: &str, messages: &Messages) -> AppResult<String> {
    required_text(
        "description",
        description,
        MAX_ITEM_DESCRIPTION_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

/// Normalises a category: trimmed, ASCII-folded, and `None` when it is empty.
///
/// `expense_items_category_chk` requires the stored value to equal its own
/// `LOWER(BTRIM(...))` and to be 1 to 64 characters, so the normalisation is
/// what makes a well-formed client value storable. An empty value — or one of
/// spaces — is the published "store `null`" case, not an error.
///
/// # Why the fold is ASCII-only
///
/// The constraint's `LOWER()` is not Unicode lower-casing. The two disagree for
/// code points outside ASCII — `U+0130` (`İ`) is the clearest example: Rust's
/// `to_lowercase` maps it to `i` plus a combining dot, while `LOWER()` may
/// leave it unchanged — so a value this function folded and the column then
/// re-checked could fail the `CHECK` and reach the client as a `500` for a body
/// the service had already accepted.
///
/// The published category examples are lower-case ASCII tokens, so this
/// function folds only `A`–`Z`, which is the range both implementations agree
/// on, and refuses a category that carries a non-ASCII character with a `422`
/// naming `category` instead of passing it to the database. That narrows the
/// accepted input, never the stored rule: every value that reaches the column
/// is one the column accepts.
pub fn normalize_category(category: &str, messages: &Messages) -> AppResult<Option<String>> {
    let trimmed = category.trim();

    if trimmed.is_empty() {
        return Ok(None);
    }

    if !trimmed.is_ascii() {
        return Err(field_error(
            "category",
            messages.validation.category_not_ascii.clone(),
            messages,
        ));
    }

    let normalised = trimmed.to_ascii_lowercase();

    if char_len(&normalised) > MAX_ITEM_CATEGORY_LENGTH {
        return Err(field_error(
            "category",
            render(
                &messages.validation.too_long,
                &[MAX_ITEM_CATEGORY_LENGTH.to_string()],
            ),
            messages,
        ));
    }

    Ok(Some(normalised))
}

/// Validates a receipt reference and returns it trimmed.
///
/// A receipt is a reference, never an upload (`ADR-0009`), so it is never
/// fetched. The accepted form is an absolute `http` or `https` URL of at most
/// [`MAX_RECEIPT_URL_LENGTH`] characters: the column's own
/// `expense_items_receipt_url_chk` requires `~* '^https?://'`, and a relative
/// path or a `javascript:`/`file:`/`data:` scheme has no meaning a client could
/// follow. The scheme comparison is case-insensitive, matching the column's
/// `~*`.
///
/// A scheme is not an absolute URL on its own. `expense_items_receipt_url_chk`
/// accepts `"https:///"`, because that string does match `^https?://`, while
/// the published rule is "an absolute `http`/`https` URL" and an empty
/// authority is not absolute. The service therefore requires a non-empty
/// authority: the character after `://` must be neither `/`, `?`, `#` nor
/// whitespace. The column is unchanged, so this check can only narrow what is
/// stored — it never lets through a value the column would refuse.
pub fn validate_receipt_url(url: &str, messages: &Messages) -> AppResult<String> {
    let trimmed = url.trim();
    let lowercase = trimmed.to_ascii_lowercase();

    let rest = if lowercase.starts_with("https://") {
        &trimmed[8..]
    } else if lowercase.starts_with("http://") {
        &trimmed[7..]
    } else {
        ""
    };

    // An empty `rest` yields `None`, which is also a refusal: `"https://"` has
    // no authority either.
    let has_authority = rest
        .chars()
        .next()
        .is_some_and(|first| !first.is_whitespace() && !matches!(first, '/' | '?' | '#'));

    if !has_authority || char_len(trimmed) > MAX_RECEIPT_URL_LENGTH {
        return Err(field_error(
            "receipt_url",
            messages.validation.receipt_url_invalid.clone(),
            messages,
        ));
    }

    Ok(trimmed.to_string())
}

/// Resolves the category tri-state into the pair the statement binds.
///
/// `(true, None)` clears the column; `(false, None)` leaves it untouched;
/// `(false, Some(value))` writes the normalised value. An empty value is the
/// published "store `null`" case, which for a `PATCH` is a clear.
fn resolve_category(
    change: (bool, Option<&str>),
    messages: &Messages,
) -> AppResult<(bool, Option<String>)> {
    match change {
        (true, _) => Ok((true, None)),
        (false, None) => Ok((false, None)),
        (false, Some(raw)) => Ok(match normalize_category(raw, messages)? {
            Some(value) => (false, Some(value)),
            None => (true, None),
        }),
    }
}

/// Resolves the receipt-reference tri-state into the pair the statement binds.
fn resolve_receipt_url(
    change: (bool, Option<&str>),
    messages: &Messages,
) -> AppResult<(bool, Option<String>)> {
    match change {
        (true, _) => Ok((true, None)),
        (false, None) => Ok((false, None)),
        (false, Some(raw)) => Ok((false, Some(validate_receipt_url(raw, messages)?))),
    }
}

/// Refuses a submission range whose start is later than its end.
///
/// Equal bounds are a valid one-day range. The violation names
/// `submitted_from`, because that is the field a client can correct, and the
/// check runs before the statement.
pub fn validate_submission_range(
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    messages: &Messages,
) -> AppResult<()> {
    if let (Some(from), Some(to)) = (from, to)
        && from > to
    {
        return Err(field_error(
            "submitted_from",
            messages.validation.date_range_order.clone(),
            messages,
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::messages::test_messages;
    use crate::error::code;

    #[test]
    fn a_currency_is_three_letters_and_is_upper_cased() {
        let messages = test_messages();

        assert_eq!(validate_currency("eur", &messages).unwrap(), "EUR");
        assert_eq!(validate_currency("  Eur  ", &messages).unwrap(), "EUR");
        assert_eq!(validate_currency("USD", &messages).unwrap(), "USD");

        for rejected in ["EU", "EURO", "1US", "EU1", "", "   ", "€UR"] {
            assert!(
                validate_currency(rejected, &messages).is_err(),
                "{rejected:?} must be refused"
            );
        }
    }

    #[test]
    fn a_reversed_submission_range_is_refused() {
        let messages = test_messages();
        let from = NaiveDate::from_ymd_opt(2026, 10, 31).expect("a valid date");
        let to = NaiveDate::from_ymd_opt(2026, 10, 1).expect("a valid date");

        let error = validate_submission_range(Some(from), Some(to), &messages)
            .expect_err("a reversed range must be refused");
        let body = error.body();
        assert_eq!(body.error.code, code::VALIDATION_FAILED);
        assert_eq!(body.error.details[0].field, "submitted_from");

        // Equal bounds are a valid one-day range.
        assert!(validate_submission_range(Some(from), Some(from), &messages).is_ok());
        // A single bound is always valid.
        assert!(validate_submission_range(Some(from), None, &messages).is_ok());
        assert!(validate_submission_range(None, Some(to), &messages).is_ok());
    }

    /// A total that `NUMERIC(19,4)` cannot hold is a `422` naming `amount`.
    ///
    /// The repository's own guard is what refuses it, in SQL, and this test
    /// pins the translation: the outcome must never reach the client as the
    /// column's `22003`, which [`crate::error::AppError::Database`] would turn
    /// into a `500`. Each item may carry fifteen integer digits, so two legal
    /// amounts already overrun the total's range.
    #[test]
    fn a_total_beyond_the_column_is_a_422_naming_the_amount() {
        let messages = test_messages();

        let error = item_refusal(ItemWrite::<()>::TotalExceedsMaximum, &messages)
            .expect_err("a total beyond the column must be refused");

        let body = error.body();
        assert_eq!(error.status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body.error.code, code::VALIDATION_FAILED);
        assert_eq!(body.error.details[0].field, "amount");
        assert_eq!(
            body.error.details[0].message,
            messages.validation.claim_total_exceeds_maximum
        );
    }

    #[test]
    fn a_blank_search_means_no_filter() {
        let messages = test_messages();

        assert_eq!(normalize_search("   ", &messages).unwrap(), None);
        assert_eq!(normalize_search("", &messages).unwrap(), None);
        assert_eq!(
            normalize_search("  berlin  ", &messages).unwrap(),
            Some("berlin".to_string())
        );

        let too_long = "x".repeat(crate::helpers::validation::MAX_SEARCH_LENGTH + 1);
        let error = normalize_search(&too_long, &messages).expect_err("an over-long term fails");
        assert_eq!(error.body().error.details[0].field, "search");
    }

    #[test]
    fn a_claim_title_is_trimmed_before_it_is_measured() {
        let messages = test_messages();
        let title = "x".repeat(MAX_TITLE_LENGTH);

        assert_eq!(
            validate_title(&format!("   {title}   "), &messages).unwrap(),
            title,
            "the surrounding spaces are not counted and are not stored"
        );

        // One character past the bound, after trimming, is refused.
        let error = validate_title(
            &format!("  {}  ", "x".repeat(MAX_TITLE_LENGTH + 1)),
            &messages,
        )
        .expect_err("an over-long title must be refused");
        assert_eq!(error.body().error.details[0].field, "title");
    }

    /// The five allowed edges, and the fifteen refused ones.
    ///
    /// The table is the whole lifecycle rule, so this test walks every pair of
    /// five statuses and four actions rather than the cases someone thought of.
    #[test]
    fn the_transition_table_allows_exactly_the_five_edges() {
        const STATUSES: [ClaimStatus; 5] = [
            ClaimStatus::Draft,
            ClaimStatus::Submitted,
            ClaimStatus::Approved,
            ClaimStatus::Rejected,
            ClaimStatus::Paid,
        ];
        const ACTIONS: [LifecycleAction; 4] = [
            LifecycleAction::Submit,
            LifecycleAction::Approve,
            LifecycleAction::Reject,
            LifecycleAction::Pay,
        ];
        // `from`, `action`, `to` — the contract's table, row for row.
        const EDGES: [(ClaimStatus, LifecycleAction, ClaimStatus); 5] = [
            (
                ClaimStatus::Draft,
                LifecycleAction::Submit,
                ClaimStatus::Submitted,
            ),
            (
                ClaimStatus::Submitted,
                LifecycleAction::Approve,
                ClaimStatus::Approved,
            ),
            (
                ClaimStatus::Submitted,
                LifecycleAction::Reject,
                ClaimStatus::Rejected,
            ),
            (
                ClaimStatus::Approved,
                LifecycleAction::Pay,
                ClaimStatus::Paid,
            ),
            // The reopen: a rejected claim goes back to `draft`, and is not
            // submitted by the same request.
            (
                ClaimStatus::Rejected,
                LifecycleAction::Submit,
                ClaimStatus::Draft,
            ),
        ];

        let mut allowed = 0;
        let mut refused = 0;

        for status in STATUSES {
            for action in ACTIONS {
                let expected = EDGES
                    .iter()
                    .find(|(from, step, _)| *from == status && *step == action)
                    .map(|(_, _, to)| *to);

                assert_eq!(
                    ensure_transition(status, action),
                    expected,
                    "{status:?} + {action:?} must match the published transition table"
                );

                match expected {
                    Some(_) => allowed += 1,
                    None => refused += 1,
                }
            }
        }

        assert_eq!(allowed, 5, "the table has exactly five allowed edges");
        assert_eq!(refused, 15, "every other pair of the twenty is refused");
    }

    /// A reviewed claim is not reopened by any action.
    ///
    /// `approved` and `paid` are the two statuses a client is most likely to
    /// want to undo, which is exactly why the table refuses both.
    #[test]
    fn a_reviewed_claim_cannot_be_reopened() {
        assert_eq!(
            ensure_transition(ClaimStatus::Approved, LifecycleAction::Submit),
            None
        );
        assert_eq!(
            ensure_transition(ClaimStatus::Paid, LifecycleAction::Submit),
            None
        );
        assert_eq!(
            ensure_transition(ClaimStatus::Paid, LifecycleAction::Approve),
            None
        );
    }

    #[test]
    fn a_receipt_must_be_an_absolute_http_url() {
        let messages = test_messages();

        for accepted in [
            "https://receipts.example.test/2026/10/20-train.pdf",
            "https://receipts.example/1.pdf",
            "http://example.test/a",
            "HTTPS://EXAMPLE.TEST/A",
        ] {
            assert_eq!(
                validate_receipt_url(accepted, &messages).unwrap(),
                accepted,
                "{accepted:?} is an absolute http(s) URL and must be accepted"
            );
        }

        for refused in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/plain,hello",
            "example.test/receipt.pdf",
            "/receipts/1.pdf",
            "https://",
            "",
            "   ",
            // A scheme with an empty authority is not an absolute URL, even
            // though `expense_items_receipt_url_chk` accepts it.
            "https:///",
            "http:///receipts/1.pdf",
            "https://?claim=7",
            "https://#receipt",
            "https:// /receipts/1.pdf",
        ] {
            let error = validate_receipt_url(refused, &messages)
                .expect_err("a receipt must be an absolute http or https URL");
            assert_eq!(error.body().error.details[0].field, "receipt_url");
        }

        // The published bound is 2048 characters, measured on the trimmed value.
        let long = format!(
            "https://example.test/{}",
            "a".repeat(MAX_RECEIPT_URL_LENGTH)
        );
        assert!(validate_receipt_url(&long, &messages).is_err());
        let at_bound = format!(
            "https://example.test/{}",
            "a".repeat(MAX_RECEIPT_URL_LENGTH - "https://example.test/".len())
        );
        assert!(validate_receipt_url(&at_bound, &messages).is_ok());
    }

    #[test]
    fn an_item_category_is_trimmed_lower_cased_and_optional() {
        let messages = test_messages();

        assert_eq!(
            normalize_category("  Travel  ", &messages).unwrap(),
            Some("travel".to_string())
        );
        assert_eq!(
            normalize_category("MEALS", &messages).unwrap(),
            Some("meals".to_string())
        );
        assert_eq!(normalize_category("   ", &messages).unwrap(), None);
        assert_eq!(normalize_category("", &messages).unwrap(), None);

        let too_long = "x".repeat(MAX_ITEM_CATEGORY_LENGTH + 1);
        let error = normalize_category(&too_long, &messages)
            .expect_err("a category over its bound must be refused");
        assert_eq!(error.body().error.details[0].field, "category");

        assert!(
            normalize_category(&"x".repeat(MAX_ITEM_CATEGORY_LENGTH), &messages).is_ok(),
            "a category at the bound must be accepted"
        );
    }

    /// The fold is ASCII-only, because `LOWER()` in the column and Rust's
    /// Unicode `to_lowercase` disagree outside ASCII: `U+0130` (`İ`) folded in
    /// Rust is not the `LOWER()` value the `CHECK` compares against, so such a
    /// value would become a `23514` and a `500` instead of a `422`.
    #[test]
    fn a_non_ascii_category_is_refused_before_it_reaches_the_database() {
        let messages = test_messages();

        for refused in [
            "\u{0130}stanbul",
            "café",
            "Ä",
            "trävel",
            "travel\u{0130}",
            "ｆｕｌｌｗｉｄｔｈ",
        ] {
            let error = normalize_category(refused, &messages)
                .expect_err("a non-ASCII category must be refused");
            let body = error.body();
            assert_eq!(error.status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(
                body.error.details[0].field, "category",
                "{refused:?} must be reported on `category`"
            );
            assert_eq!(
                body.error.details[0].message, messages.validation.category_not_ascii,
                "{refused:?} must carry the ASCII-only message"
            );
        }

        // A blank value is still the published "store null" case, not an
        // error, and it is checked before the ASCII rule.
        assert_eq!(normalize_category("   ", &messages).unwrap(), None);
    }
}
