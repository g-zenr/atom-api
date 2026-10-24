//! Company, application and event use cases: validation, the stage machine and
//! the ownership mapping.
//!
//! Every method takes `owner` first, exactly as `ClaimService` does, so an
//! operation cannot be called without saying whose row is meant. The service
//! never compares identifiers and never returns `403`: a repository miss becomes
//! [`AppError::NotFound`] with the same message for "does not exist" and
//! "belongs to somebody else" (`ADR-0005` §3).
//!
//! Three rules hold in this order:
//!
//! 1. **Validate the whole body first, then touch the database.** A request
//!    that is both malformed and aimed at a foreign resource is a `422`, not a
//!    `404`: the body never had to be read.
//! 2. **Every miss is a catalogue message**, never a message assembled here.
//! 3. **The stage decision belongs to the statement, not to a preceding read.**
//!    The guarded `UPDATE` carries the set of stages the request may come from,
//!    so two concurrent moves cannot both pass a check and then both write. A
//!    zero-row result is explained afterwards, on the error path, by one
//!    owner-scoped read that changes no state.
//!
//! # What the database owns and what this file owns
//!
//! | Rule | Where |
//! | ---- | ----- |
//! | A company name is unique per owner, ignoring case | The functional index `companies_user_id_lower_name_unique_idx`; the service only maps `23505` to `409` and never compares names itself |
//! | The seven stages | The `job_application_status` enum |
//! | The twelve edges and the three terminal stages | [`ALLOWED_MOVES`], in this file — a `CHECK` sees one row and cannot see the previous stage |
//! | A company with applications cannot be deleted | One `DELETE … AND NOT EXISTS`, in the repository |
//! | The next-action task belongs to the caller | [`JobService`], because `tasks` has no `user_id` for a composite key |
//! | `role_title`, `summary` and `source` are trimmed | This file: the schema documents "trimmed" but has no `CHECK` for those three, so an untrimmed value would be stored untrimmed rather than refused |
//!
//! Money is validated here but never parsed: [`validate_money`] is a character
//! check that returns the string unchanged, and there is no addition,
//! subtraction or rounding of a salary anywhere (`ADR-0007`). The one
//! computation is the `min <= max` **comparison**, which is a comparison of
//! decimal strings, not arithmetic on their value.

use std::cmp::Ordering;
use std::sync::Arc;

use chrono::NaiveDate;
use uuid::Uuid;

use crate::config::messages::Messages;
use crate::config::settings::PaginationSettings;
use crate::error::{AppError, AppResult};
use crate::helpers::validation::validate_money;
use crate::helpers::{bounded_text, field_error, required_text, validate_search};
use crate::job_applications::dto::{
    ApplicationQuery, ApplicationResponse, CompanyQuery, CompanyResponse, CreateApplicationRequest,
    CreateCompanyRequest, CreateEventRequest, EventResponse, MAX_COMPANY_NAME_LENGTH,
    MAX_COMPANY_NOTES_LENGTH, MAX_COMPANY_WEBSITE_LENGTH, MAX_DESCRIPTION_LENGTH,
    MAX_EVENT_DETAILS_LENGTH, MAX_EVENT_SUMMARY_LENGTH, MAX_JOB_URL_LENGTH, MAX_ROLE_TITLE_LENGTH,
    MAX_SOURCE_LENGTH, UpdateApplicationRequest, UpdateCompanyRequest, UpdateEventRequest,
};
use crate::job_applications::models::ApplicationStatus;
use crate::job_applications::repository::{
    ApplicationListFilter, ApplicationPatch, CompanyListFilter, CompanyPatch, CompanyWrite,
    EventPatch, JobRepository, NewApplication, NewCompany, NewEvent, SalaryRange,
};
use crate::pagination::{Page, PaginationQuery};
use crate::utils::text::char_len;

/// The twelve stage moves the machine allows.
///
/// This constant is the **only** definition of the edge table. Both
/// [`ensure_transition`] and [`permitted_sources`] read it, so the `409` a
/// client receives and the predicate the write carries cannot disagree — which
/// is what a second, hand-written copy would eventually do.
///
/// It is a `match` on the pair and not a rank comparison, because `rejected`
/// and `withdrawn` are reachable from four different stages and a numeric rank
/// cannot express that. `offer`, `rejected` and `withdrawn` appear only as a
/// destination, which is what makes them terminal.
pub const ALLOWED_MOVES: [(ApplicationStatus, ApplicationStatus); 12] = [
    // The forward sequence.
    (ApplicationStatus::Wishlist, ApplicationStatus::Applied),
    (ApplicationStatus::Applied, ApplicationStatus::Screening),
    (ApplicationStatus::Screening, ApplicationStatus::Interview),
    (ApplicationStatus::Interview, ApplicationStatus::Offer),
    // The two exits from each of the four live stages.
    (ApplicationStatus::Wishlist, ApplicationStatus::Rejected),
    (ApplicationStatus::Applied, ApplicationStatus::Rejected),
    (ApplicationStatus::Screening, ApplicationStatus::Rejected),
    (ApplicationStatus::Interview, ApplicationStatus::Rejected),
    (ApplicationStatus::Wishlist, ApplicationStatus::Withdrawn),
    (ApplicationStatus::Applied, ApplicationStatus::Withdrawn),
    (ApplicationStatus::Screening, ApplicationStatus::Withdrawn),
    (ApplicationStatus::Interview, ApplicationStatus::Withdrawn),
];

/// What sending a stage means, when the machine accepts it.
///
/// The distinction exists because the seven self-pairs are **accepted and
/// change nothing**: they are not refused, so [`ensure_transition`] cannot
/// answer `Option<()>` without losing the difference between "moved" and
/// "already there".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    /// The requested stage is the current one. Accepted; the row is unchanged.
    NoOp,
    /// An allowed edge. The stage changes.
    Move,
}

/// The transition table: what sending `next` from `current` means.
///
/// `None` means the pair is not in the table, which the service reports as
/// `409 conflict`. Of the 49 ordered pairs, 12 are [`Transition::Move`], the 7
/// self-pairs are [`Transition::NoOp`], and the remaining 30 are `None`.
///
/// The success path does **not** call this function to decide whether to write:
/// the decision is the `status = ANY(...)` predicate of the guarded `UPDATE`, so
/// a caller cannot slip between a check and the write. This function is the
/// single definition of the table that the refusal explains, and the unit test
/// below is the exhaustive proof of it.
pub fn ensure_transition(
    current: ApplicationStatus,
    next: ApplicationStatus,
) -> Option<Transition> {
    if current == next {
        return Some(Transition::NoOp);
    }

    ALLOWED_MOVES
        .iter()
        .any(|(from, to)| *from == current && *to == next)
        .then_some(Transition::Move)
}

/// The stages a request may come from when it asks for `next`.
///
/// It is the requested stage itself — the accepted no-op — plus every `from`
/// of an allowed edge into it. The result is what the guarded `UPDATE` binds, so
/// it must be derived from [`ALLOWED_MOVES`] rather than written out again.
pub fn permitted_sources(next: ApplicationStatus) -> Vec<ApplicationStatus> {
    let mut sources = vec![next];

    for (from, to) in ALLOWED_MOVES {
        if to == next && !sources.contains(&from) {
            sources.push(from);
        }
    }

    sources
}

/// The stage an application is born in.
///
/// Only `wishlist`, or an omitted stage, is accepted: every later stage is
/// reached through the machine, and a create that accepted one would bypass it.
/// This is the whole rule, in one function, so the `422` and its unit test
/// cannot drift from the route that applies it.
pub fn ensure_initial_stage(
    requested: Option<ApplicationStatus>,
    messages: &Messages,
) -> AppResult<ApplicationStatus> {
    match requested {
        None | Some(ApplicationStatus::Wishlist) => Ok(ApplicationStatus::Wishlist),
        Some(_) => Err(field_error(
            "status",
            messages.validation.stage_must_start_at_wishlist.clone(),
            messages,
        )),
    }
}

/// Company, application and event use cases.
#[derive(Debug, Clone)]
pub struct JobService {
    repository: JobRepository,
    messages: Arc<Messages>,
    pagination: PaginationSettings,
}

impl JobService {
    /// Builds the service.
    pub fn new(
        repository: JobRepository,
        messages: Arc<Messages>,
        pagination: PaginationSettings,
    ) -> Self {
        Self {
            repository,
            messages,
            pagination,
        }
    }

    // ---------------------------------------------------------------- companies

    /// Lists the owner's companies, alphabetically.
    pub async fn list_companies(
        &self,
        owner: Uuid,
        query: CompanyQuery,
        pagination: PaginationQuery,
    ) -> AppResult<Page<CompanyResponse>> {
        let search = query
            .search
            .as_deref()
            .map(|term| normalize_search(term, &self.messages))
            .transpose()?
            .flatten();

        let filter = CompanyListFilter {
            search: search.as_deref(),
        };

        let limit = pagination.effective_limit(&self.pagination);
        let offset = pagination.effective_offset();

        let (companies, total) = self
            .repository
            .list_companies_owned(owner, &filter, limit, offset)
            .await?;

        let items = companies.into_iter().map(CompanyResponse::from).collect();

        Ok(Page::new(items, limit, offset, total))
    }

    /// Creates a company for the given owner.
    ///
    /// A name the owner already uses, ignoring case, is a `409`: the functional
    /// unique index refuses it and the repository reports the `23505` rather
    /// than a database error, so the answer is the same whether the clash is
    /// found on the first try or by two concurrent creates.
    pub async fn create_company(
        &self,
        owner: Uuid,
        request: CreateCompanyRequest,
    ) -> AppResult<CompanyResponse> {
        let name = validate_company_name(&request.name, &self.messages)?;
        let website = request
            .website
            .as_deref()
            .map(|website| validate_website(website, &self.messages))
            .transpose()?;
        let notes = validate_company_notes(&request.notes, &self.messages)?;

        let outcome = self
            .repository
            .insert_company(&NewCompany {
                id: Uuid::new_v4(),
                owner,
                name: &name,
                website: website.as_deref(),
                notes: &notes,
            })
            .await?;

        match outcome {
            CompanyWrite::Applied(company) => Ok(CompanyResponse::from(company)),
            CompanyWrite::DuplicateName => self.company_name_taken(),
        }
    }

    /// Loads one of the owner's companies.
    pub async fn get_company(&self, owner: Uuid, company_id: Uuid) -> AppResult<CompanyResponse> {
        let company = self
            .repository
            .find_company_owned(company_id, owner)
            .await?
            .ok_or_else(|| AppError::NotFound(self.messages.resources.company_not_found.clone()))?;

        Ok(CompanyResponse::from(company))
    }

    /// Applies a partial update to one of the owner's companies.
    ///
    /// An omitted field is left untouched; `website` may be cleared with an
    /// explicit `null`. A rename onto another company's name is a `409`, and a
    /// rename onto the company's **own** name — in any casing — is accepted,
    /// because the index excludes the row being updated.
    pub async fn update_company(
        &self,
        owner: Uuid,
        company_id: Uuid,
        request: UpdateCompanyRequest,
    ) -> AppResult<CompanyResponse> {
        let name = request
            .name
            .as_deref()
            .map(|name| validate_company_name(name, &self.messages))
            .transpose()?;
        let (clear_website, website) = resolve_website(request.website_change(), &self.messages)?;
        let notes = request
            .notes
            .as_deref()
            .map(|notes| validate_company_notes(notes, &self.messages))
            .transpose()?;

        let outcome = self
            .repository
            .update_company_owned(
                company_id,
                owner,
                &CompanyPatch {
                    name: name.as_deref(),
                    clear_website,
                    website: website.as_deref(),
                    notes: notes.as_deref(),
                },
            )
            .await?;

        match outcome {
            CompanyWrite::Applied(Some(company)) => Ok(CompanyResponse::from(company)),
            CompanyWrite::Applied(None) => Err(AppError::NotFound(
                self.messages.resources.company_not_found.clone(),
            )),
            CompanyWrite::DuplicateName => self.company_name_taken(),
        }
    }

    /// Deletes one of the owner's companies, only when nothing references it.
    ///
    /// The refusal does not delete the applications to make the delete succeed:
    /// they are records the client did not ask to delete.
    pub async fn delete_company(&self, owner: Uuid, company_id: Uuid) -> AppResult<()> {
        if self
            .repository
            .delete_company_if_unused(company_id, owner)
            .await?
        {
            return Ok(());
        }

        // The statement refused without saying which of the three reasons
        // applies. One owner-scoped read names it; nothing is written either
        // way.
        if self.repository.company_is_owned(company_id, owner).await? {
            return Err(AppError::Conflict(
                self.messages.conflicts.company_has_applications.clone(),
            ));
        }

        Err(AppError::NotFound(
            self.messages.resources.company_not_found.clone(),
        ))
    }

    // ------------------------------------------------------------- applications

    /// Lists the owner's applications, newest first.
    pub async fn list_applications(
        &self,
        owner: Uuid,
        query: ApplicationQuery,
        pagination: PaginationQuery,
    ) -> AppResult<Page<ApplicationResponse>> {
        let search = query
            .search
            .as_deref()
            .map(|term| normalize_search(term, &self.messages))
            .transpose()?
            .flatten();
        validate_applied_range(query.applied_from, query.applied_to, &self.messages)?;

        let filter = ApplicationListFilter {
            status: query.status,
            company_id: query.company_id,
            search: search.as_deref(),
            applied_from: query.applied_from,
            applied_to: query.applied_to,
        };

        let limit = pagination.effective_limit(&self.pagination);
        let offset = pagination.effective_offset();

        let (applications, total) = self
            .repository
            .list_applications_owned(owner, &filter, limit, offset)
            .await?;

        let items = applications
            .into_iter()
            .map(ApplicationResponse::from)
            .collect();

        Ok(Page::new(items, limit, offset, total))
    }

    /// Creates an application in `wishlist` for the given owner.
    ///
    /// The stage is fixed at birth: only `wishlist`, or an omitted stage, is
    /// accepted, so a client cannot skip the machine by creating the row in the
    /// stage it wanted. Every later change goes through
    /// [`Self::update_application`].
    pub async fn create_application(
        &self,
        owner: Uuid,
        request: CreateApplicationRequest,
    ) -> AppResult<ApplicationResponse> {
        ensure_initial_stage(request.status, &self.messages)?;

        let role_title = validate_role_title(&request.role_title, &self.messages)?;
        let source = request
            .source
            .as_deref()
            .map(|source| validate_source(source, &self.messages))
            .transpose()?;
        let description = validate_description(&request.description, &self.messages)?;
        let job_url = request
            .job_url
            .as_deref()
            .map(|url| validate_job_url(url, &self.messages))
            .transpose()?;
        let salary = validate_salary_range(
            request.salary_min.as_deref(),
            request.salary_max.as_deref(),
            request.salary_currency.as_deref(),
            &self.messages,
        )?;

        let application = self
            .repository
            .insert_application(&NewApplication {
                id: Uuid::new_v4(),
                owner,
                company_id: request.company_id,
                role_title: &role_title,
                source: source.as_deref(),
                applied_on: request.applied_on,
                salary: &salary,
                job_url: job_url.as_deref(),
                description: &description,
                next_action_task_id: request.next_action_task_id,
            })
            .await?;

        // The insert refuses both links at once, so a `None` has to be named.
        let Some(application) = application else {
            return Err(self
                .explain_insert_refusal(owner, request.company_id, request.next_action_task_id)
                .await?);
        };

        Ok(ApplicationResponse::from(application))
    }

    /// Loads one of the owner's applications.
    pub async fn get_application(
        &self,
        owner: Uuid,
        application_id: Uuid,
    ) -> AppResult<ApplicationResponse> {
        let application = self
            .repository
            .find_application_owned(application_id, owner)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(self.messages.resources.application_not_found.clone())
            })?;

        Ok(ApplicationResponse::from(application))
    }

    /// Applies a partial update to one of the owner's applications, moving its
    /// stage when the request names one.
    ///
    /// The whole request is **one** `UPDATE`, which is why a refused move
    /// writes none of the other fields: either every column of the statement is
    /// written or none is. The stage decision is the statement's
    /// `status = ANY(...)` predicate; a zero-row result is explained afterwards,
    /// on the error path, by one owner-scoped read.
    ///
    /// The three salary columns are resolved against the stored triple when the
    /// request touches any of them, because they are validated together:
    /// clearing the minimum has to know whether a maximum and a currency are
    /// still there. That read resolves values; it never decides whether the row
    /// exists.
    pub async fn update_application(
        &self,
        owner: Uuid,
        application_id: Uuid,
        request: UpdateApplicationRequest,
    ) -> AppResult<ApplicationResponse> {
        let role_title = request
            .role_title
            .as_deref()
            .map(|title| validate_role_title(title, &self.messages))
            .transpose()?;
        let (clear_source, source) = resolve_source(request.source_change(), &self.messages)?;
        let description = request
            .description
            .as_deref()
            .map(|description| validate_description(description, &self.messages))
            .transpose()?;
        let (clear_job_url, job_url) = resolve_job_url(request.job_url_change(), &self.messages)?;
        let (clear_applied_on, applied_on) = request.applied_on_change();
        let (clear_next_action, next_action_task_id) = request.next_action_change();

        let salary = if request.touches_salary() {
            match self
                .repository
                .find_salary_owned(application_id, owner)
                .await?
            {
                Some(current) => Some(resolve_salary(&current, &request, &self.messages)?),
                // No row: the guarded write below answers it. Resolving a triple
                // for a row that does not exist would invent one.
                None => None,
            }
        } else {
            None
        };

        let sources = request.status.map(permitted_sources);

        let updated = self
            .repository
            .update_application_owned(
                application_id,
                owner,
                &ApplicationPatch {
                    company_id: request.company_id,
                    role_title: role_title.as_deref(),
                    clear_source,
                    source: source.as_deref(),
                    status: request.status,
                    clear_applied_on,
                    applied_on,
                    salary: salary.as_ref(),
                    clear_job_url,
                    job_url: job_url.as_deref(),
                    description: description.as_deref(),
                    clear_next_action,
                    next_action_task_id,
                },
                sources,
            )
            .await?;

        let Some(application) = updated else {
            return Err(self
                .explain_update_refusal(owner, application_id, &request)
                .await?);
        };

        Ok(ApplicationResponse::from(application))
    }

    /// Deletes one of the owner's applications. Its events go with it.
    pub async fn delete_application(&self, owner: Uuid, application_id: Uuid) -> AppResult<()> {
        if self
            .repository
            .delete_application_owned(application_id, owner)
            .await?
        {
            return Ok(());
        }

        Err(AppError::NotFound(
            self.messages.resources.application_not_found.clone(),
        ))
    }

    // ------------------------------------------------------------------- events

    /// Lists the events of one of the owner's applications, newest first.
    pub async fn list_events(
        &self,
        owner: Uuid,
        application_id: Uuid,
        pagination: PaginationQuery,
    ) -> AppResult<Page<EventResponse>> {
        let limit = pagination.effective_limit(&self.pagination);
        let offset = pagination.effective_offset();

        let (events, total) = self
            .repository
            .list_events_owned(application_id, owner, limit, offset)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(self.messages.resources.application_not_found.clone())
            })?;

        let items = events.into_iter().map(EventResponse::from).collect();

        Ok(Page::new(items, limit, offset, total))
    }

    /// Adds one event to one of the owner's applications.
    ///
    /// `occurred_at` may be in the future: an interview is scheduled before it
    /// happens, so the field means "belongs at this point on the timeline", not
    /// "is in the past".
    pub async fn create_event(
        &self,
        owner: Uuid,
        application_id: Uuid,
        request: CreateEventRequest,
    ) -> AppResult<EventResponse> {
        let summary = validate_event_summary(&request.summary, &self.messages)?;
        let details = validate_event_details(&request.details, &self.messages)?;

        let event = self
            .repository
            .insert_event(&NewEvent {
                id: Uuid::new_v4(),
                owner,
                application_id,
                kind: request.kind,
                occurred_at: request.occurred_at,
                summary: &summary,
                details: &details,
            })
            .await?
            .ok_or_else(|| {
                AppError::NotFound(self.messages.resources.application_not_found.clone())
            })?;

        Ok(EventResponse::from(event))
    }

    /// Corrects one event of one of the owner's applications, in place.
    ///
    /// No timestamp is written and the response carries none: `created_at` is
    /// the only row timestamp an event keeps (`A-D1-1`).
    pub async fn update_event(
        &self,
        owner: Uuid,
        application_id: Uuid,
        event_id: Uuid,
        request: UpdateEventRequest,
    ) -> AppResult<EventResponse> {
        let summary = request
            .summary
            .as_deref()
            .map(|summary| validate_event_summary(summary, &self.messages))
            .transpose()?;
        let details = request
            .details
            .as_deref()
            .map(|details| validate_event_details(details, &self.messages))
            .transpose()?;

        let event = self
            .repository
            .update_event_owned(
                application_id,
                event_id,
                owner,
                &EventPatch {
                    kind: request.kind,
                    occurred_at: request.occurred_at,
                    summary: summary.as_deref(),
                    details: details.as_deref(),
                },
            )
            .await?
            .ok_or_else(|| AppError::NotFound(self.messages.resources.event_not_found.clone()))?;

        Ok(EventResponse::from(event))
    }

    /// Deletes one event of one of the owner's applications.
    pub async fn delete_event(
        &self,
        owner: Uuid,
        application_id: Uuid,
        event_id: Uuid,
    ) -> AppResult<()> {
        if self
            .repository
            .delete_event_owned(application_id, event_id, owner)
            .await?
        {
            return Ok(());
        }

        Err(AppError::NotFound(
            self.messages.resources.event_not_found.clone(),
        ))
    }

    // ------------------------------------------------------------- error paths

    /// The `409` a duplicate company name produces.
    fn company_name_taken<T>(&self) -> AppResult<T> {
        Err(AppError::Conflict(
            self.messages.conflicts.company_name_taken.clone(),
        ))
    }

    /// Names which link of a refused application insert was the problem.
    ///
    /// The insert refuses the company and the task in one statement, so the
    /// answer is not visible in its result. Both links are `404`, in this order:
    /// the company, which the request must always carry, then the optional
    /// task. Anything else would be a row the statement's own predicates allow,
    /// which cannot happen; it is reported as an internal error rather than
    /// guessed at.
    async fn explain_insert_refusal(
        &self,
        owner: Uuid,
        company_id: Uuid,
        next_action_task_id: Option<Uuid>,
    ) -> AppResult<AppError> {
        if !self.repository.company_is_owned(company_id, owner).await? {
            return Ok(AppError::NotFound(
                self.messages.resources.company_not_found.clone(),
            ));
        }

        if let Some(task_id) = next_action_task_id
            && !self.repository.task_is_owned(task_id, owner).await?
        {
            return Ok(AppError::NotFound(
                self.messages.resources.task_not_found.clone(),
            ));
        }

        Ok(AppError::Internal(
            "the application insert refused a row its predicates allow".to_string(),
        ))
    }

    /// Names why a guarded application update affected no row.
    ///
    /// The guarded statement is the decision; this read only names its reason,
    /// so the order matters:
    ///
    /// 1. the application is absent or foreign → `404`;
    /// 2. a company in the body is foreign → `404`;
    /// 3. a task in the body is foreign → `404`;
    /// 4. the requested stage is not reachable from the stored one → `409`;
    /// 5. anything else is a concurrent move that landed between the statement
    ///    and this read, which is also a `409`.
    async fn explain_update_refusal(
        &self,
        owner: Uuid,
        application_id: Uuid,
        request: &UpdateApplicationRequest,
    ) -> AppResult<AppError> {
        let Some(_) = self
            .repository
            .find_application_owned(application_id, owner)
            .await?
        else {
            return Ok(AppError::NotFound(
                self.messages.resources.application_not_found.clone(),
            ));
        };

        if let Some(company_id) = request.company_id
            && !self.repository.company_is_owned(company_id, owner).await?
        {
            return Ok(AppError::NotFound(
                self.messages.resources.company_not_found.clone(),
            ));
        }

        let (clear_next_action, next_action_task_id) = request.next_action_change();
        if !clear_next_action
            && let Some(task_id) = next_action_task_id
            && !self.repository.task_is_owned(task_id, owner).await?
        {
            return Ok(AppError::NotFound(
                self.messages.resources.task_not_found.clone(),
            ));
        }

        Ok(AppError::Conflict(
            self.messages.conflicts.stage_transition_not_allowed.clone(),
        ))
    }
}

// --------------------------------------------------------------------- company
// validation

/// Validates a company name.
///
/// The name is trimmed before it is measured, so 200 characters surrounded by
/// spaces is accepted and stored trimmed. The trim is a hard requirement:
/// `companies_name_trimmed_chk` refuses an untrimmed value, and a trailing space
/// would make `"Acme "` a second `"Acme"` that the folded unique index cannot
/// see as a duplicate.
///
/// # Why nothing is folded here
///
/// The comparison is folded by `companies_user_id_lower_name_unique_idx`, in the
/// database, over `LOWER(name)`. The service does not lower-case anything, so
/// the two cannot disagree about which code points fold — the failure mode the
/// commitment category in `D2` had to avoid with an ASCII-only fold. The stored
/// casing is preserved because a company name is a display form.
pub fn validate_company_name(name: &str, messages: &Messages) -> AppResult<String> {
    required_text(
        "name",
        name,
        MAX_COMPANY_NAME_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

/// Validates a company website.
///
/// A website is a reference, never a fetch (`ADR-0009`), so it is never
/// requested. The accepted form is an absolute `http` or `https` URL of at most
/// [`MAX_COMPANY_WEBSITE_LENGTH`] characters.
pub fn validate_website(url: &str, messages: &Messages) -> AppResult<String> {
    validate_http_url(
        "website",
        url,
        MAX_COMPANY_WEBSITE_LENGTH,
        &messages.validation.website_invalid,
        messages,
    )
}

/// Validates company notes.
///
/// The notes are kept verbatim, so `""` clears them and a value of spaces is
/// stored as it was sent. That mirrors the column, which is
/// `NOT NULL DEFAULT ''` and measured by `companies_notes_length_chk`.
pub fn validate_company_notes(notes: &str, messages: &Messages) -> AppResult<String> {
    bounded_text(
        "notes",
        notes,
        MAX_COMPANY_NOTES_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

/// Resolves the website tri-state into the pair the statement binds.
fn resolve_website(
    change: (bool, Option<&str>),
    messages: &Messages,
) -> AppResult<(bool, Option<String>)> {
    match change {
        (true, _) => Ok((true, None)),
        (false, None) => Ok((false, None)),
        (false, Some(raw)) => Ok((false, Some(validate_website(raw, messages)?))),
    }
}

// ---------------------------------------------------------------- application
// validation

/// Validates a role title.
///
/// Trimmed before it is measured. `role_title` has no `CHECK` for the trim in
/// `0005`, so this service rule is the only thing that keeps an untrimmed title
/// out of the column.
pub fn validate_role_title(title: &str, messages: &Messages) -> AppResult<String> {
    required_text(
        "role_title",
        title,
        MAX_ROLE_TITLE_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

/// Validates an application description. Kept verbatim; `""` clears it.
pub fn validate_description(description: &str, messages: &Messages) -> AppResult<String> {
    bounded_text(
        "description",
        description,
        MAX_DESCRIPTION_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

/// Validates a source label and returns it trimmed.
///
/// `job_applications_source_chk` requires the stored value to equal its own
/// `BTRIM(...)` and to be 1 to 64 characters, so a blank value has no legal
/// spelling: `null` clears the column and `""` is a `422`.
pub fn validate_source(source: &str, messages: &Messages) -> AppResult<String> {
    required_text(
        "source",
        source,
        MAX_SOURCE_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

/// Validates a job posting reference and returns it trimmed.
///
/// Stored as text and never fetched, for the same SSRF reason as the company
/// website.
pub fn validate_job_url(url: &str, messages: &Messages) -> AppResult<String> {
    validate_http_url(
        "job_url",
        url,
        MAX_JOB_URL_LENGTH,
        &messages.validation.job_url_invalid,
        messages,
    )
}

/// Resolves the source tri-state into the pair the statement binds.
fn resolve_source(
    change: (bool, Option<&str>),
    messages: &Messages,
) -> AppResult<(bool, Option<String>)> {
    match change {
        (true, _) => Ok((true, None)),
        (false, None) => Ok((false, None)),
        (false, Some(raw)) => Ok((false, Some(validate_source(raw, messages)?))),
    }
}

/// Resolves the job-reference tri-state into the pair the statement binds.
fn resolve_job_url(
    change: (bool, Option<&str>),
    messages: &Messages,
) -> AppResult<(bool, Option<String>)> {
    match change {
        (true, _) => Ok((true, None)),
        (false, None) => Ok((false, None)),
        (false, Some(raw)) => Ok((false, Some(validate_job_url(raw, messages)?))),
    }
}

/// Refuses a date range whose start is later than its end.
///
/// Equal bounds are a valid one-day range. The violation names `applied_from`,
/// because that is the field a client can correct, and the check runs before
/// the statement.
pub fn validate_applied_range(
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    messages: &Messages,
) -> AppResult<()> {
    if let (Some(from), Some(to)) = (from, to)
        && from > to
    {
        return Err(field_error(
            "applied_from",
            messages.validation.applied_range_order.clone(),
            messages,
        ));
    }

    Ok(())
}

/// Validates and normalises a currency code.
///
/// Trimmed and upper-cased, then exactly three ASCII letters, exactly as
/// `validate_currency` does for a claim. `salary_currency` is `CHAR(3)` with
/// `job_applications_salary_currency_chk (salary_currency ~ '^[A-Z]{3}$')`, so
/// the written value is always upper case; normalising here makes a lower-case
/// client input a normalisation instead of a constraint violation, and an
/// invalid one a `422` before any statement runs.
pub fn validate_currency(currency: &str, messages: &Messages) -> AppResult<String> {
    let normalised = currency.trim().to_ascii_uppercase();

    if normalised.len() != 3 || !normalised.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(field_error(
            "salary_currency",
            messages.validation.currency_invalid.clone(),
            messages,
        ));
    }

    Ok(normalised)
}

/// Validates the whole salary triple of a create request.
///
/// Three rules, all published in `api.md` §9:
///
/// * a bound without a currency is a `422` naming `salary_currency`, because a
///   number whose currency is unknown is worse than no number;
/// * a currency with neither bound is a `422` naming `salary_currency`, for the
///   same reason inverted;
/// * a minimum above the maximum is a `422` naming `salary_min`.
///
/// Equal bounds are accepted: the rule is `<=`, which is also what
/// `job_applications_salary_range_chk` enforces.
pub fn validate_salary_range(
    min: Option<&str>,
    max: Option<&str>,
    currency: Option<&str>,
    messages: &Messages,
) -> AppResult<SalaryRange> {
    let min = min
        .map(|value| validate_money("salary_min", value, messages))
        .transpose()?;
    let max = max
        .map(|value| validate_money("salary_max", value, messages))
        .transpose()?;
    let currency = currency
        .map(|value| validate_currency(value, messages))
        .transpose()?;

    if min.is_none() && max.is_none() {
        if currency.is_some() {
            return Err(field_error(
                "salary_currency",
                messages.validation.currency_without_bound.clone(),
                messages,
            ));
        }

        return Ok(SalaryRange::default());
    }

    let Some(currency) = currency else {
        return Err(field_error(
            "salary_currency",
            messages.validation.currency_required.clone(),
            messages,
        ));
    };

    validate_salary_order(min.as_deref(), max.as_deref(), messages)?;

    Ok(SalaryRange {
        min,
        max,
        currency: Some(currency),
    })
}

/// Resolves the salary triple of a `PATCH` against the stored one.
///
/// The three columns are one value, so they are resolved together:
///
/// * an omitted field keeps the stored value, an explicit `null` clears it;
/// * clearing **both** bounds clears the currency, which is the published rule;
/// * an explicit currency with no bound left is still a `422`, not a silent
///   no-op, because a client that sends one is asking for something the column
///   cannot express;
/// * a non-null currency is required whenever a bound survives;
/// * the resolved pair must be ordered.
///
/// The callers only reach this when the request touches a salary field; see
/// [`JobService::update_application`] for why that is the only case that needs
/// the stored triple read.
pub fn resolve_salary(
    current: &SalaryRange,
    request: &UpdateApplicationRequest,
    messages: &Messages,
) -> AppResult<SalaryRange> {
    let min = resolve_bound(
        &request.salary_min,
        current.min.as_deref(),
        "salary_min",
        messages,
    )?;
    let max = resolve_bound(
        &request.salary_max,
        current.max.as_deref(),
        "salary_max",
        messages,
    )?;

    let currency = if min.is_none() && max.is_none() {
        if matches!(&request.salary_currency, Some(Some(_))) {
            return Err(field_error(
                "salary_currency",
                messages.validation.currency_without_bound.clone(),
                messages,
            ));
        }

        None
    } else {
        let resolved = match &request.salary_currency {
            None => current.currency.clone(),
            Some(None) => None,
            Some(Some(value)) => Some(validate_currency(value, messages)?),
        };

        let Some(currency) = resolved else {
            return Err(field_error(
                "salary_currency",
                messages.validation.currency_required.clone(),
                messages,
            ));
        };

        Some(currency)
    };

    validate_salary_order(min.as_deref(), max.as_deref(), messages)?;

    Ok(SalaryRange { min, max, currency })
}

/// Resolves one bound of the salary triple.
fn resolve_bound(
    change: &Option<Option<String>>,
    stored: Option<&str>,
    field: &str,
    messages: &Messages,
) -> AppResult<Option<String>> {
    match change {
        None => Ok(stored.map(str::to_string)),
        Some(None) => Ok(None),
        Some(Some(value)) => Ok(Some(validate_money(field, value, messages)?)),
    }
}

/// Refuses a range whose minimum is above its maximum.
fn validate_salary_order(
    min: Option<&str>,
    max: Option<&str>,
    messages: &Messages,
) -> AppResult<()> {
    if let (Some(min), Some(max)) = (min, max)
        && compare_money(min, max) == Ordering::Greater
    {
        return Err(field_error(
            "salary_min",
            messages.validation.salary_range_order.clone(),
            messages,
        ));
    }

    Ok(())
}

/// Compares two validated four-decimal money strings, without parsing them.
///
/// Both inputs are in the form [`validate_money`] accepts —
/// `^[0-9]{1,15}\.[0-9]{4}$` — so comparing the left-padded integer part and
/// then the fraction is exact. Leading zeros have to be trimmed first: `"05.0000"`
/// and `"5.0000"` are the same value and a plain string comparison would order
/// them by length.
///
/// Nothing is converted to `f32`, `f64` or an integer, and nothing is
/// arithmetically combined; the *value* of the money is never computed
/// (`ADR-0007`). This is the one place the API compares two salaries, and it
/// exists because `min <= max` is a rule a client can break and the database's
/// `CHECK` would answer as a `23514`, which is a `500`.
pub fn compare_money(left: &str, right: &str) -> Ordering {
    money_key(left).cmp(&money_key(right))
}

/// The sort key of one validated money string.
fn money_key(value: &str) -> String {
    let (integer, fraction) = match value.split_once('.') {
        Some((integer, fraction)) => (integer, fraction),
        None => (value, ""),
    };

    let significant = integer.trim_start_matches('0');
    let significant = if significant.is_empty() {
        "0"
    } else {
        significant
    };

    format!("{significant:0>15}.{fraction}")
}

// --------------------------------------------------------------- event
// validation

/// Validates an event summary.
///
/// Trimmed before it is measured. `application_events_summary_length_chk` bounds
/// the column but has no trim clause, so this rule is the only thing that keeps
/// an untrimmed summary out of the table.
pub fn validate_event_summary(summary: &str, messages: &Messages) -> AppResult<String> {
    required_text(
        "summary",
        summary,
        MAX_EVENT_SUMMARY_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

/// Validates event details. Kept verbatim; `""` clears them.
pub fn validate_event_details(details: &str, messages: &Messages) -> AppResult<String> {
    bounded_text(
        "details",
        details,
        MAX_EVENT_DETAILS_LENGTH,
        &messages.validation.too_long,
        messages,
    )
}

// -------------------------------------------------------------------- shared

/// The one absolute-URL rule, for the website and the job reference.
///
/// The column's own `CHECK` requires `~* '^https?://'`; that is not an absolute
/// URL on its own, because `"https:///"` matches it while having no authority.
/// The published rule is "an absolute `http`/`https` URL", so the character
/// after `://` must be neither `/`, `?`, `#` nor whitespace. The check can only
/// narrow what the column accepts, never widen it.
fn validate_http_url(
    field: &str,
    url: &str,
    max: usize,
    invalid: &str,
    messages: &Messages,
) -> AppResult<String> {
    let trimmed = url.trim();
    let lowercase = trimmed.to_ascii_lowercase();

    let rest = if lowercase.starts_with("https://") {
        &trimmed["https://".len()..]
    } else if lowercase.starts_with("http://") {
        &trimmed["http://".len()..]
    } else {
        ""
    };

    let has_authority = rest
        .chars()
        .next()
        .is_some_and(|first| !first.is_whitespace() && !matches!(first, '/' | '?' | '#'));

    if !has_authority || char_len(trimmed) > max {
        return Err(field_error(field, invalid.to_string(), messages));
    }

    Ok(trimmed.to_string())
}

/// Normalises a search term: trimmed, and `None` when it is empty.
///
/// An empty term means "no filter", which is the published rule for this
/// parameter. It is not an error.
pub fn normalize_search(term: &str, messages: &Messages) -> AppResult<Option<String>> {
    let trimmed = validate_search(term, messages)?;

    Ok(Some(trimmed).filter(|value| !value.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::messages::test_messages;
    use crate::error::code;

    const STAGES: [ApplicationStatus; 7] = [
        ApplicationStatus::Wishlist,
        ApplicationStatus::Applied,
        ApplicationStatus::Screening,
        ApplicationStatus::Interview,
        ApplicationStatus::Offer,
        ApplicationStatus::Rejected,
        ApplicationStatus::Withdrawn,
    ];

    /// The whole stage machine, pair by pair.
    ///
    /// Seven stages, forty-nine ordered pairs. The test walks every pair rather
    /// than the cases someone thought of, and pins the three counts the contract
    /// states: **12** allowed edges, **7** accepted no-ops and **30** refusals.
    /// A self-pair is a no-op, not a refusal — that is the trap, and the
    /// `NoOp` arm below is what catches it.
    #[test]
    fn the_stage_machine_allows_exactly_the_twelve_edges() {
        // `from`, `to` — the contract's table, row for row.
        const EDGES: [(ApplicationStatus, ApplicationStatus); 12] = [
            (ApplicationStatus::Wishlist, ApplicationStatus::Applied),
            (ApplicationStatus::Applied, ApplicationStatus::Screening),
            (ApplicationStatus::Screening, ApplicationStatus::Interview),
            (ApplicationStatus::Interview, ApplicationStatus::Offer),
            (ApplicationStatus::Wishlist, ApplicationStatus::Rejected),
            (ApplicationStatus::Applied, ApplicationStatus::Rejected),
            (ApplicationStatus::Screening, ApplicationStatus::Rejected),
            (ApplicationStatus::Interview, ApplicationStatus::Rejected),
            (ApplicationStatus::Wishlist, ApplicationStatus::Withdrawn),
            (ApplicationStatus::Applied, ApplicationStatus::Withdrawn),
            (ApplicationStatus::Screening, ApplicationStatus::Withdrawn),
            (ApplicationStatus::Interview, ApplicationStatus::Withdrawn),
        ];

        assert_eq!(EDGES, ALLOWED_MOVES, "the constant is the published table");

        let mut moves = 0;
        let mut no_ops = 0;
        let mut refused = 0;

        for current in STAGES {
            for next in STAGES {
                let expected = if current == next {
                    Some(Transition::NoOp)
                } else if EDGES
                    .iter()
                    .any(|(from, to)| *from == current && *to == next)
                {
                    Some(Transition::Move)
                } else {
                    None
                };

                let actual = ensure_transition(current, next);

                assert_eq!(
                    actual, expected,
                    "{current:?} -> {next:?} must match the published transition table"
                );

                match actual {
                    Some(Transition::Move) => moves += 1,
                    Some(Transition::NoOp) => no_ops += 1,
                    None => refused += 1,
                }
            }
        }

        assert_eq!(moves, 12, "the table has exactly twelve allowed edges");
        assert_eq!(no_ops, 7, "every self-pair is an accepted no-op");
        assert_eq!(refused, 30, "every other pair of the forty-nine is refused");
        assert_eq!(moves + no_ops + refused, 49);
    }

    /// Nothing leaves a terminal stage, and re-sending it is still a no-op.
    ///
    /// `offer`, `rejected` and `withdrawn` are terminal, which the enum cannot
    /// express and no `CHECK` can either.
    #[test]
    fn the_three_terminal_stages_have_no_exit() {
        for terminal in [
            ApplicationStatus::Offer,
            ApplicationStatus::Rejected,
            ApplicationStatus::Withdrawn,
        ] {
            for next in STAGES {
                if next == terminal {
                    assert_eq!(
                        ensure_transition(terminal, next),
                        Some(Transition::NoOp),
                        "re-sending {terminal:?} is a no-op, not an exit"
                    );
                    continue;
                }

                assert_eq!(
                    ensure_transition(terminal, next),
                    None,
                    "{terminal:?} is terminal, so {terminal:?} -> {next:?} must be refused"
                );
            }
        }
    }

    /// The set the guarded `UPDATE` binds is exactly the sources the table
    /// allows, so the `409` and the statement cannot disagree.
    #[test]
    fn the_permitted_sources_are_the_edges_into_the_stage_plus_itself() {
        for next in STAGES {
            let sources = permitted_sources(next);

            assert!(
                sources.contains(&next),
                "the self-pair of {next:?} must be permitted, or the no-op would be a 409"
            );

            for current in STAGES {
                let permitted = sources.contains(&current);
                let expected = ensure_transition(current, next).is_some();

                assert_eq!(
                    permitted, expected,
                    "{current:?} -> {next:?}: the statement's sources must match the table"
                );
            }
        }
    }

    /// A create bypasses the machine unless the stage is fixed at birth.
    ///
    /// Every one of the seven stages is walked: only `wishlist` and an omitted
    /// value are accepted, and the other six are the `422` that stops
    /// `POST /job-applications` from writing an application straight into
    /// `interview`.
    #[test]
    fn a_create_accepts_only_wishlist_or_an_omitted_stage() {
        let messages = test_messages();

        assert_eq!(
            ensure_initial_stage(None, &messages).unwrap(),
            ApplicationStatus::Wishlist,
            "an omitted stage defaults to wishlist"
        );
        assert_eq!(
            ensure_initial_stage(Some(ApplicationStatus::Wishlist), &messages).unwrap(),
            ApplicationStatus::Wishlist,
            "wishlist is the one stage a create may name"
        );

        for refused in STAGES {
            if refused == ApplicationStatus::Wishlist {
                continue;
            }

            let error = ensure_initial_stage(Some(refused), &messages)
                .expect_err("only wishlist is accepted on create");
            let body = error.body();

            assert_eq!(error.status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(body.error.code, code::VALIDATION_FAILED);
            assert_eq!(body.error.details[0].field, "status");
            assert_eq!(
                body.error.details[0].message, messages.validation.stage_must_start_at_wishlist,
                "{refused:?} must be refused with the create-stage message"
            );
        }
    }

    #[test]
    fn a_company_name_is_trimmed_and_keeps_its_casing() {
        let messages = test_messages();

        assert_eq!(
            validate_company_name("  Acme GmbH  ", &messages).unwrap(),
            "Acme GmbH",
            "the name is stored trimmed with the casing the owner typed"
        );

        // The comparison is folded by the database index over `LOWER(name)`, so
        // the service must not fold anything: two names that differ only in
        // case stay two distinct strings here and collide only in the index.
        assert_ne!(
            validate_company_name("Acme", &messages).unwrap(),
            validate_company_name("acme", &messages).unwrap(),
            "the service must not lower-case a company name"
        );

        let blank = validate_company_name("   ", &messages).expect_err("a blank name is refused");
        assert_eq!(blank.body().error.details[0].field, "name");

        let error = validate_company_name(&"x".repeat(MAX_COMPANY_NAME_LENGTH + 1), &messages)
            .expect_err("an over-long name must be refused");
        assert_eq!(error.body().error.details[0].field, "name");

        assert!(
            validate_company_name(&"x".repeat(MAX_COMPANY_NAME_LENGTH), &messages).is_ok(),
            "a name at the bound must be accepted"
        );
    }

    #[test]
    fn a_website_and_a_job_url_must_be_absolute_http_urls() {
        let messages = test_messages();

        for accepted in [
            "https://acme.example.test",
            "http://example.test/jobs/42",
            "HTTPS://EXAMPLE.TEST/A",
            // An authority of `.` is nonsense but it is a non-empty authority,
            // and the published contract accepts it verbatim.
            "https://.",
        ] {
            assert_eq!(
                validate_website(accepted, &messages).unwrap(),
                accepted,
                "{accepted:?} is an absolute http(s) URL and must be accepted"
            );
            assert!(validate_job_url(accepted, &messages).is_ok());
        }

        for refused in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/plain,hello",
            "acme.example.test",
            "/jobs/42",
            "https://",
            "https:///jobs/42",
            "https://?role=1",
            "https://#role",
            "",
            "   ",
        ] {
            let error = validate_website(refused, &messages)
                .expect_err("a reference must be an absolute http or https URL");
            assert_eq!(error.body().error.details[0].field, "website");

            let error = validate_job_url(refused, &messages)
                .expect_err("a job reference must be an absolute http or https URL");
            assert_eq!(error.body().error.details[0].field, "job_url");
        }

        let long = format!("https://example.test/{}", "a".repeat(MAX_JOB_URL_LENGTH));
        assert!(validate_job_url(&long, &messages).is_err());
    }

    #[test]
    fn a_salary_bound_without_a_currency_is_refused() {
        let messages = test_messages();

        let minimum_only = validate_salary_range(Some("60000.0000"), None, None, &messages)
            .expect_err("a bound with no currency is refused");
        assert_eq!(
            minimum_only.body().error.details[0].field,
            "salary_currency"
        );

        let maximum_only = validate_salary_range(None, Some("75000.0000"), None, &messages)
            .expect_err("a bound with no currency is refused");
        assert_eq!(
            maximum_only.body().error.details[0].field,
            "salary_currency"
        );

        let currency_only = validate_salary_range(None, None, Some("EUR"), &messages)
            .expect_err("a currency with no bound is refused");
        assert_eq!(
            currency_only.body().error.details[0].field,
            "salary_currency"
        );

        let complete = validate_salary_range(
            Some("60000.0000"),
            Some("75000.0000"),
            Some("EUR"),
            &messages,
        )
        .unwrap();
        assert_eq!(
            complete,
            SalaryRange {
                min: Some("60000.0000".to_string()),
                max: Some("75000.0000".to_string()),
                currency: Some("EUR".to_string()),
            }
        );

        // All three null is the published "no salary" shape.
        assert_eq!(
            validate_salary_range(None, None, None, &messages).unwrap(),
            SalaryRange::default()
        );

        // A lower-case code is normalised, not refused: the wire form is upper
        // case and the column's CHECK requires it.
        assert_eq!(
            validate_salary_range(Some("1.0000"), None, Some("eur"), &messages)
                .unwrap()
                .currency
                .as_deref(),
            Some("EUR")
        );
    }

    #[test]
    fn a_salary_minimum_above_the_maximum_is_refused() {
        let messages = test_messages();

        let error = validate_salary_range(
            Some("80000.0000"),
            Some("75000.0000"),
            Some("EUR"),
            &messages,
        )
        .expect_err("an inverted range must be refused");
        assert_eq!(error.body().error.details[0].field, "salary_min");
        assert_eq!(error.body().error.code, code::VALIDATION_FAILED);

        // Equal bounds are accepted: the rule is `<=`.
        assert!(
            validate_salary_range(
                Some("75000.0000"),
                Some("75000.0000"),
                Some("EUR"),
                &messages
            )
            .is_ok()
        );
    }

    /// The comparison is exact for every value `validate_money` accepts,
    /// including the spelling with leading zeros that the column normalises
    /// away on the way back out.
    #[test]
    fn the_money_comparison_is_exact_and_ignores_leading_zeros() {
        assert_eq!(compare_money("1.0000", "2.0000"), Ordering::Less);
        assert_eq!(compare_money("2.0000", "1.0000"), Ordering::Greater);
        assert_eq!(compare_money("2.0000", "2.0000"), Ordering::Equal);
        assert_eq!(compare_money("05.0000", "5.0000"), Ordering::Equal);
        assert_eq!(compare_money("005.0000", "10.0000"), Ordering::Less);
        assert_eq!(
            compare_money("999999999999999.9999", "999999999999999.9998"),
            Ordering::Greater
        );
        assert_eq!(compare_money("0.0000", "0.0001"), Ordering::Less);
        assert_eq!(compare_money("60000.0000", "75000.0000"), Ordering::Less);
    }

    /// A `PATCH` that touches one bound is validated against the stored triple,
    /// because the three columns are one value.
    #[test]
    fn a_patched_bound_is_validated_against_the_stored_triple() {
        let messages = test_messages();
        let stored = SalaryRange {
            min: Some("60000.0000".to_string()),
            max: Some("75000.0000".to_string()),
            currency: Some("EUR".to_string()),
        };

        // Raising the minimum above the stored maximum is refused.
        let raised: UpdateApplicationRequest =
            serde_json::from_str(r#"{"salary_min":"80000.0000"}"#).expect("a valid patch");
        let error = resolve_salary(&stored, &raised, &messages)
            .expect_err("the resolved pair would be inverted");
        assert_eq!(error.body().error.details[0].field, "salary_min");

        // A minimum alone keeps the stored currency.
        let kept: UpdateApplicationRequest =
            serde_json::from_str(r#"{"salary_min":"65000.0000"}"#).expect("a valid patch");
        let resolved = resolve_salary(&stored, &kept, &messages).unwrap();
        assert_eq!(resolved.currency.as_deref(), Some("EUR"));
        assert_eq!(resolved.min.as_deref(), Some("65000.0000"));
        assert_eq!(resolved.max.as_deref(), Some("75000.0000"));

        // Clearing both clears the currency, even though it was not sent.
        let cleared: UpdateApplicationRequest =
            serde_json::from_str(r#"{"salary_min":null,"salary_max":null}"#)
                .expect("a valid patch");
        assert_eq!(
            resolve_salary(&stored, &cleared, &messages).unwrap(),
            SalaryRange::default()
        );

        // Clearing the last remaining bound leaves no bound at all, so the
        // currency goes with it: the rule is stated over the resolved triple,
        // which is also the only shape `job_applications_salary_currency_
        // required_chk` will store.
        let only_max = SalaryRange {
            min: None,
            max: Some("75000.0000".to_string()),
            currency: Some("EUR".to_string()),
        };
        let clear_max: UpdateApplicationRequest =
            serde_json::from_str(r#"{"salary_max":null}"#).expect("a valid patch");
        assert_eq!(
            resolve_salary(&only_max, &clear_max, &messages).unwrap(),
            SalaryRange::default()
        );

        // Asking for a currency while no bound survives is the published `422`:
        // the client is requesting something the column cannot express.
        let currency_only: UpdateApplicationRequest =
            serde_json::from_str(r#"{"salary_currency":"USD"}"#).expect("a valid patch");
        let error = resolve_salary(&SalaryRange::default(), &currency_only, &messages)
            .expect_err("a currency with no bound has no meaning");
        assert_eq!(error.body().error.details[0].field, "salary_currency");
    }

    #[test]
    fn a_blank_search_means_no_filter() {
        let messages = test_messages();

        assert_eq!(normalize_search("   ", &messages).unwrap(), None);
        assert_eq!(
            normalize_search("  acme  ", &messages).unwrap(),
            Some("acme".to_string())
        );
    }

    #[test]
    fn a_reversed_applied_range_is_refused() {
        let messages = test_messages();
        let from = NaiveDate::from_ymd_opt(2026, 10, 31).expect("a valid date");
        let to = NaiveDate::from_ymd_opt(2026, 10, 1).expect("a valid date");

        let error = validate_applied_range(Some(from), Some(to), &messages)
            .expect_err("a reversed range must be refused");
        assert_eq!(error.body().error.details[0].field, "applied_from");

        assert!(validate_applied_range(Some(from), Some(from), &messages).is_ok());
        assert!(validate_applied_range(None, Some(to), &messages).is_ok());
    }

    #[test]
    fn a_source_is_trimmed_and_has_no_blank_spelling() {
        let messages = test_messages();

        assert_eq!(
            validate_source("  referral  ", &messages).unwrap(),
            "referral"
        );

        let blank = validate_source("   ", &messages).expect_err("a blank source is refused");
        assert_eq!(blank.body().error.details[0].field, "source");

        let long = validate_source(&"x".repeat(MAX_SOURCE_LENGTH + 1), &messages)
            .expect_err("an over-long source is refused");
        assert_eq!(long.body().error.details[0].field, "source");
    }
}
