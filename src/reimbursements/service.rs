//! Claim use cases: validation, the project rule, and the ownership mapping.
//!
//! Every method takes `owner` first, exactly as `ProjectService` does, so a
//! claim operation cannot be called without saying whose claim is meant. The
//! service never compares identifiers and never returns `403`: a repository
//! miss becomes [`AppError::NotFound`] with the same message for "does not
//! exist" and "belongs to somebody else" (`ADR-0005` §3).
//!
//! Two rules hold in this order:
//!
//! 1. **Validate the whole body first, then touch the database.** A request
//!    that is both malformed and aimed at a foreign resource is a `422`, not a
//!    `404`: the body never had to be read.
//! 2. **Every miss is a catalogue message**, never a message assembled here.
//!
//! No money is validated here, because no request body of this module carries
//! money (ADR-0007; design document §4.2).

use std::sync::Arc;

use chrono::NaiveDate;
use uuid::Uuid;

use crate::config::messages::Messages;
use crate::config::settings::PaginationSettings;
use crate::error::{AppError, AppResult};
use crate::helpers::{bounded_text, field_error, required_text, validate_search};
use crate::pagination::{Page, PaginationQuery};
use crate::reimbursements::dto::{
    ClaimDetailResponse, ClaimQuery, ClaimResponse, CreateClaimRequest, MAX_DESCRIPTION_LENGTH,
    MAX_TITLE_LENGTH, UpdateClaimRequest,
};
use crate::reimbursements::repository::{ClaimListFilter, ClaimRepository};

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
    pub async fn get(&self, owner: Uuid, claim_id: Uuid) -> AppResult<ClaimDetailResponse> {
        let claim = self
            .repository
            .find_owned(claim_id, owner)
            .await?
            .ok_or_else(|| AppError::NotFound(self.messages.resources.claim_not_found.clone()))?;

        let items = self.repository.list_items_owned(claim_id, owner).await?;

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

        let items = self.repository.list_items_owned(claim.id, owner).await?;

        Ok(ClaimDetailResponse::new(claim, items))
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
}
