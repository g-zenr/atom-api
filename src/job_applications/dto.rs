//! Public contracts for the `job_applications` module.
//!
//! The shapes mirror [`docs/features/job-applications/api.md`] §2.1–§2.5 field
//! for field. Three properties are carried by the types themselves rather than
//! by a convention a caller has to remember:
//!
//! * **`user_id` never reaches the wire.** Every response type is built from a
//!   row model and drops the owner column, exactly as `ClaimResponse` does.
//! * **Money is a `String` with four decimals.** `salary_min` and `salary_max`
//!   are `NUMERIC(19,4)::text` on the server side and are never parsed,
//!   formatted or arithmetically combined in Rust (`ADR-0007`). A JSON number
//!   is therefore impossible to produce from these types, and the OpenAPI
//!   components say the four-decimal pattern. A JSON number on the way **in**
//!   is refused by the shared extractor with a `422`.
//! * **A nullable column has three request states, not two.** `website`,
//!   `source`, `applied_on`, the salary fields, `job_url` and
//!   `next_action_task_id` are `Option<Option<T>>` on the update requests:
//!   an omitted key leaves the column untouched, `null` clears it, a value
//!   writes it. Collapsing that to `Option<T>` would make "leave alone" and
//!   "clear" the same instruction, which is the `PATCH` bug this API is
//!   specified to avoid.
//!
//! [`EventResponse`] deliberately carries **no `updated_at`** ([`api.md`] §2.3,
//! decision `A-D1-1`): a timeline entry records what the owner says happened,
//! not when the row was last written. The absence is the decision, not an
//! oversight, and a reviewer must not "restore" the field for consistency with
//! the other two resources.
//!
//! [`docs/features/job-applications/api.md`]: ../../docs/features/job-applications/api.md
//! [`api.md`]: ../../docs/features/job-applications/api.md

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::job_applications::models::{
    ApplicationEvent, ApplicationEventKind, ApplicationStatus, Company, JobApplication,
};
use crate::pagination::{Page, Pagination, PaginationQuery};

/// Maximum accepted company name length.
///
/// Mirrors `companies_name_length_chk` (`CHECK (CHAR_LENGTH(name) BETWEEN 1 AND
/// 200)`) in `migrations/0005_job_applications.sql`. The migration is the
/// enforced copy; this constant is the application-side mirror of it.
pub const MAX_COMPANY_NAME_LENGTH: usize = 200;

/// Maximum accepted company website length.
///
/// Mirrors `companies_website_chk`, which bounds the column and requires an
/// `http`/`https` scheme. The reference is never fetched (`ADR-0009`).
pub const MAX_COMPANY_WEBSITE_LENGTH: usize = 2048;

/// Maximum accepted company notes length.
///
/// Mirrors `companies_notes_length_chk` (`CHECK (CHAR_LENGTH(notes) <= 5000)`).
pub const MAX_COMPANY_NOTES_LENGTH: usize = 5_000;

/// Maximum accepted role title length.
///
/// Mirrors `job_applications_role_title_length_chk`. The schema documents the
/// value as trimmed but has **no** `CHECK` for the trim — `companies` and
/// `job_applications.source` do, `role_title` does not — so the trimming is a
/// service rule and an untrimmed value would be stored untrimmed rather than
/// refused.
pub const MAX_ROLE_TITLE_LENGTH: usize = 200;

/// Maximum accepted source label length.
///
/// Mirrors the length half of `job_applications_source_chk`; the other half is
/// the trim, which the service performs and the column re-checks.
pub const MAX_SOURCE_LENGTH: usize = 64;

/// Maximum accepted application description length.
///
/// Mirrors `job_applications_description_length_chk`
/// (`CHECK (CHAR_LENGTH(description) <= 10000)`).
pub const MAX_DESCRIPTION_LENGTH: usize = 10_000;

/// Maximum accepted job posting reference length.
///
/// Mirrors `job_applications_job_url_chk`, which bounds the column and requires
/// an `http`/`https` scheme. The posting is stored as text and never fetched.
pub const MAX_JOB_URL_LENGTH: usize = 2048;

/// Maximum accepted event summary length.
///
/// Mirrors `application_events_summary_length_chk`
/// (`CHECK (CHAR_LENGTH(summary) BETWEEN 1 AND 200)`). The trim is a service
/// rule; the column has no `CHECK` for it.
pub const MAX_EVENT_SUMMARY_LENGTH: usize = 200;

/// Maximum accepted event detail length.
///
/// Mirrors `application_events_details_length_chk`
/// (`CHECK (CHAR_LENGTH(details) <= 10000)`).
pub const MAX_EVENT_DETAILS_LENGTH: usize = 10_000;

/// The public representation of a company.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CompanyResponse {
    /// Stable public identifier.
    pub id: Uuid,
    /// The stored casing of the name, trimmed.
    pub name: String,
    /// Optional absolute `http`/`https` reference, never fetched.
    pub website: Option<String>,
    /// Free text; may be empty.
    pub notes: String,
    /// Creation timestamp, RFC 3339 UTC.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp, RFC 3339 UTC.
    pub updated_at: DateTime<Utc>,
}

/// Builds the public representation of a company. `user_id` is dropped.
impl From<Company> for CompanyResponse {
    fn from(company: Company) -> Self {
        Self {
            id: company.id,
            name: company.name,
            website: company.website,
            notes: company.notes,
            created_at: company.created_at,
            updated_at: company.updated_at,
        }
    }
}

/// The response body of `GET /api/v1/companies`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CompanyPage {
    /// The companies on this page.
    pub items: Vec<CompanyResponse>,
    /// Pagination metadata.
    pub pagination: Pagination,
}

impl From<Page<CompanyResponse>> for CompanyPage {
    fn from(page: Page<CompanyResponse>) -> Self {
        Self {
            items: page.items,
            pagination: page.pagination,
        }
    }
}

/// The public representation of a job application.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ApplicationResponse {
    /// Stable public identifier.
    pub id: Uuid,
    /// The employer, always owned by the same account as the application.
    pub company_id: Uuid,
    /// Trimmed role title, 1 to 200 characters.
    pub role_title: String,
    /// Where the lead came from; a label, not a controlled value.
    pub source: Option<String>,
    /// The current stage of the seven-stage machine.
    pub status: ApplicationStatus,
    /// The calendar day the application was sent, `YYYY-MM-DD`.
    pub applied_on: Option<NaiveDate>,
    /// Optional lower bound, as a four-decimal string.
    #[schema(
        format = "decimal",
        pattern = "^[0-9]{1,15}\\.[0-9]{4}$",
        example = "60000.0000"
    )]
    pub salary_min: Option<String>,
    /// Optional upper bound, as a four-decimal string.
    #[schema(
        format = "decimal",
        pattern = "^[0-9]{1,15}\\.[0-9]{4}$",
        example = "75000.0000"
    )]
    pub salary_max: Option<String>,
    /// ISO 4217 code, present exactly when a bound is.
    pub salary_currency: Option<String>,
    /// Optional posting reference, never fetched.
    pub job_url: Option<String>,
    /// Free text; may be empty.
    pub description: String,
    /// The optional next-action task; cleared when the task is deleted.
    pub next_action_task_id: Option<Uuid>,
    /// Creation timestamp, RFC 3339 UTC.
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp, RFC 3339 UTC.
    pub updated_at: DateTime<Utc>,
}

/// Builds the public representation of an application. `user_id` is dropped.
impl From<JobApplication> for ApplicationResponse {
    fn from(application: JobApplication) -> Self {
        Self {
            id: application.id,
            company_id: application.company_id,
            role_title: application.role_title,
            source: application.source,
            status: application.status,
            applied_on: application.applied_on,
            salary_min: application.salary_min,
            salary_max: application.salary_max,
            salary_currency: application.salary_currency,
            job_url: application.job_url,
            description: application.description,
            next_action_task_id: application.next_action_task_id,
            created_at: application.created_at,
            updated_at: application.updated_at,
        }
    }
}

/// The response body of `GET /api/v1/job-applications`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ApplicationPage {
    /// The applications on this page.
    pub items: Vec<ApplicationResponse>,
    /// Pagination metadata.
    pub pagination: Pagination,
}

impl From<Page<ApplicationResponse>> for ApplicationPage {
    fn from(page: Page<ApplicationResponse>) -> Self {
        Self {
            items: page.items,
            pagination: page.pagination,
        }
    }
}

/// The public representation of one timeline entry.
///
/// There is **no `updated_at`**; see the module docs.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EventResponse {
    /// Stable identifier, meaningful only inside its application.
    pub id: Uuid,
    /// The application this event belongs to.
    pub application_id: Uuid,
    /// What kind of entry this is.
    pub kind: ApplicationEventKind,
    /// The instant the event belongs at on the timeline; may be in the future.
    pub occurred_at: DateTime<Utc>,
    /// Trimmed summary, 1 to 200 characters.
    pub summary: String,
    /// Free text; may be empty.
    pub details: String,
    /// The row's creation instant, distinct from `occurred_at`.
    pub created_at: DateTime<Utc>,
}

/// Builds the public representation of an event. `user_id` is dropped.
impl From<ApplicationEvent> for EventResponse {
    fn from(event: ApplicationEvent) -> Self {
        Self {
            id: event.id,
            application_id: event.application_id,
            kind: event.kind,
            occurred_at: event.occurred_at,
            summary: event.summary,
            details: event.details,
            created_at: event.created_at,
        }
    }
}

/// The response body of `GET /api/v1/job-applications/{application_id}/events`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EventPage {
    /// The events on this page, newest first.
    pub items: Vec<EventResponse>,
    /// Pagination metadata.
    pub pagination: Pagination,
}

impl From<Page<EventResponse>> for EventPage {
    fn from(page: Page<EventResponse>) -> Self {
        Self {
            items: page.items,
            pagination: page.pagination,
        }
    }
}

/// Request body for creating a company.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateCompanyRequest {
    /// Company name, trimmed to 1 to 200 characters and unique per owner
    /// ignoring case.
    #[schema(example = "Acme GmbH")]
    pub name: String,
    /// Optional absolute `http` or `https` reference. Never fetched.
    #[schema(example = "https://acme.example.test")]
    pub website: Option<String>,
    /// Free text. Defaults to an empty string.
    #[schema(example = "Referred by Dana")]
    #[serde(default)]
    pub notes: String,
}

/// Request body for partially updating a company.
///
/// Every field is optional and an omitted one is left untouched.
/// `website` is a tri-state: an omitted key leaves the column alone, `null`
/// clears it, a value writes it. `name` and `notes` are not nullable columns,
/// so `""` clears `notes` while an empty `name` is a validation failure.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateCompanyRequest {
    /// New name, trimmed to 1 to 200 characters. A collision with another
    /// company of the caller is a `409`.
    #[schema(example = "Acme GmbH")]
    pub name: Option<String>,
    /// The website as a tri-state: absent leaves it, `null` clears it.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>, nullable)]
    pub website: Option<Option<String>>,
    /// New notes; `""` clears them.
    #[schema(example = "")]
    pub notes: Option<String>,
}

impl UpdateCompanyRequest {
    /// The website as `(clear, value)`.
    ///
    /// A single `COALESCE($n, website)` cannot express "set to `NULL`", because
    /// `NULL` already means "leave alone", which is why the boolean is part of
    /// the pair.
    pub fn website_change(&self) -> (bool, Option<&str>) {
        match &self.website {
            None => (false, None),
            Some(None) => (true, None),
            Some(Some(value)) => (false, Some(value)),
        }
    }
}

/// Request body for creating an application.
///
/// `status` accepts **only `wishlist` or an omitted value**; any other stage is
/// a `422`, because an application starts at the beginning of the machine and
/// reaches a later stage through the transition table. Without that rule the
/// machine could be bypassed at birth.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateApplicationRequest {
    /// The employer. It must belong to the caller, otherwise `404`.
    pub company_id: Uuid,
    /// Role title, trimmed to 1 to 200 characters.
    #[schema(example = "Backend engineer")]
    pub role_title: String,
    /// Where the lead came from, at most 64 characters.
    #[schema(example = "referral")]
    pub source: Option<String>,
    /// The initial stage. Only `wishlist` is accepted.
    pub status: Option<ApplicationStatus>,
    /// The calendar day the application was sent, `YYYY-MM-DD`.
    #[schema(example = "2026-10-02")]
    pub applied_on: Option<NaiveDate>,
    /// Optional lower bound, as a four-decimal string.
    #[schema(
        format = "decimal",
        pattern = "^[0-9]{1,15}\\.[0-9]{4}$",
        example = "60000.0000"
    )]
    pub salary_min: Option<String>,
    /// Optional upper bound, as a four-decimal string.
    #[schema(
        format = "decimal",
        pattern = "^[0-9]{1,15}\\.[0-9]{4}$",
        example = "75000.0000"
    )]
    pub salary_max: Option<String>,
    /// ISO 4217 code. Required when either bound is present, and refused when
    /// neither is.
    #[schema(example = "EUR")]
    pub salary_currency: Option<String>,
    /// Optional absolute `http` or `https` reference. Never fetched.
    #[schema(example = "https://acme.example.test/jobs/42")]
    pub job_url: Option<String>,
    /// Free text. Defaults to an empty string.
    #[schema(example = "Platform team, Rust and PostgreSQL.")]
    #[serde(default)]
    pub description: String,
    /// The next-action task. It must be the caller's, otherwise `404`.
    pub next_action_task_id: Option<Uuid>,
}

/// Request body for partially updating an application.
///
/// Every field is optional and an omitted one is left untouched.
/// `company_id`, `role_title` and `description` are not nullable columns, so an
/// empty `role_title` is a validation failure while `""` clears `description`.
/// The other six are tri-states: absent leaves the column alone, `null` clears
/// it.
///
/// `status` is a **move through the stage machine**, not a plain assignment: a
/// pair the machine does not allow is a `409`, and the current stage sent again
/// is accepted as a no-op.
///
/// The three salary fields are validated together, against the stored triple,
/// so a `PATCH` that changes one bound still has to leave a consistent range.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateApplicationRequest {
    /// A corrected company. It must belong to the caller, otherwise `404`.
    pub company_id: Option<Uuid>,
    /// New role title, trimmed to 1 to 200 characters.
    #[schema(example = "Senior backend engineer")]
    pub role_title: Option<String>,
    /// The source label as a tri-state: absent leaves it, `null` clears it.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>, nullable)]
    pub source: Option<Option<String>>,
    /// The requested stage. A move the machine does not allow is a `409`.
    pub status: Option<ApplicationStatus>,
    /// The day the application was sent, as a tri-state.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<NaiveDate>, nullable)]
    pub applied_on: Option<Option<NaiveDate>>,
    /// The lower bound as a tri-state: absent leaves it, `null` clears it.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(
        value_type = Option<String>,
        nullable,
        format = "decimal",
        pattern = "^[0-9]{1,15}\\.[0-9]{4}$",
        example = "65000.0000"
    )]
    pub salary_min: Option<Option<String>>,
    /// The upper bound as a tri-state: absent leaves it, `null` clears it.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(
        value_type = Option<String>,
        nullable,
        format = "decimal",
        pattern = "^[0-9]{1,15}\\.[0-9]{4}$",
        example = "80000.0000"
    )]
    pub salary_max: Option<Option<String>>,
    /// The currency as a tri-state. Clearing both bounds also clears it.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>, nullable)]
    pub salary_currency: Option<Option<String>>,
    /// The job posting reference as a tri-state.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>, nullable)]
    pub job_url: Option<Option<String>>,
    /// New description; `""` clears it.
    #[schema(example = "")]
    pub description: Option<String>,
    /// The next-action task as a tri-state. A UUID must be the caller's.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<Uuid>, nullable)]
    pub next_action_task_id: Option<Option<Uuid>>,
}

impl UpdateApplicationRequest {
    /// The source label as `(clear, value)`.
    pub fn source_change(&self) -> (bool, Option<&str>) {
        match &self.source {
            None => (false, None),
            Some(None) => (true, None),
            Some(Some(value)) => (false, Some(value)),
        }
    }

    /// The job posting reference as `(clear, value)`.
    pub fn job_url_change(&self) -> (bool, Option<&str>) {
        match &self.job_url {
            None => (false, None),
            Some(None) => (true, None),
            Some(Some(value)) => (false, Some(value)),
        }
    }

    /// The next-action task as `(clear, value)`.
    pub fn next_action_change(&self) -> (bool, Option<Uuid>) {
        match self.next_action_task_id {
            None => (false, None),
            Some(None) => (true, None),
            Some(Some(id)) => (false, Some(id)),
        }
    }

    /// The `applied_on` day as `(clear, value)`.
    pub fn applied_on_change(&self) -> (bool, Option<NaiveDate>) {
        match self.applied_on {
            None => (false, None),
            Some(None) => (true, None),
            Some(Some(day)) => (false, Some(day)),
        }
    }

    /// Whether the request carries any salary field at all.
    ///
    /// The three columns are validated together, so the service has to resolve
    /// them together: an untouched triple needs no stored read, a touched one
    /// does.
    pub fn touches_salary(&self) -> bool {
        self.salary_min.is_some() || self.salary_max.is_some() || self.salary_currency.is_some()
    }
}

/// Request body for adding one event to an application.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateEventRequest {
    /// What kind of entry this is.
    pub kind: ApplicationEventKind,
    /// The instant the event belongs at. RFC 3339 with a time zone; a future
    /// value is accepted, because an interview is scheduled before it happens.
    #[schema(example = "2026-10-14T13:00:00Z")]
    pub occurred_at: DateTime<Utc>,
    /// Summary, trimmed to 1 to 200 characters.
    #[schema(example = "Technical interview with the platform team")]
    pub summary: String,
    /// Free text, at most 10 000 characters. Defaults to an empty string.
    #[schema(example = "Two exercises, Rust and SQL.")]
    #[serde(default)]
    pub details: String,
}

/// Request body for correcting one event.
///
/// All four content fields are optional and an omitted one is left untouched.
/// The event has no `updated_at` and this request writes no timestamp: a
/// timeline entry records what the owner says happened (`A-D1-1`).
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateEventRequest {
    /// The corrected kind.
    pub kind: Option<ApplicationEventKind>,
    /// The corrected instant.
    #[schema(example = "2026-10-15T09:00:00Z")]
    pub occurred_at: Option<DateTime<Utc>>,
    /// The corrected summary, trimmed to 1 to 200 characters.
    #[schema(example = "Technical interview — rescheduled")]
    pub summary: Option<String>,
    /// The corrected detail; `""` clears it.
    #[schema(example = "")]
    pub details: Option<String>,
}

/// Query parameters accepted by `GET /api/v1/companies`.
///
/// Only the feature filters live here. The pagination parameters live in
/// [`PaginationQuery`], and the handler reads both halves from a single
/// [`CompaniesListQuery`]: declaring `limit` and `offset` in both places would
/// publish them twice in the generated OpenAPI document.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CompanyQuery {
    /// Case-insensitive substring over the name. At most 200 characters.
    #[param(example = "acme")]
    pub search: Option<String>,
}

/// The whole query string of `GET /api/v1/companies`, deserialized once.
///
/// [`Self::into_parts`] splits the result into the pair the service layer works
/// with.
///
/// The fields are written out instead of composing [`CompanyQuery`] and
/// [`PaginationQuery`] with `#[serde(flatten)]`. The extractor deserializes with
/// `serde_urlencoded`, whose map access yields strings, so a flattened
/// `Option<i64>` would be asked to deserialize itself from a string and would
/// fail at run time, on a real request, instead of at compile time. The
/// duplication is the price of parsing the string once.
///
/// It derives `Deserialize` only, and is deliberately absent from the generated
/// OpenAPI document: the published parameter set stays [`CompanyQuery`] and
/// [`PaginationQuery`], which the handler's `utoipa` attribute names.
#[derive(Debug, Clone, Deserialize)]
pub struct CompaniesListQuery {
    /// Maximum number of companies to return.
    pub limit: Option<i64>,
    /// Number of companies to skip.
    pub offset: Option<i64>,
    /// Case-insensitive substring over the name.
    pub search: Option<String>,
}

impl CompaniesListQuery {
    /// Splits the parsed query into its filter and pagination halves.
    pub fn into_parts(self) -> (CompanyQuery, PaginationQuery) {
        let Self {
            limit,
            offset,
            search,
        } = self;

        (CompanyQuery { search }, PaginationQuery { limit, offset })
    }
}

/// Query parameters accepted by `GET /api/v1/job-applications`.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ApplicationQuery {
    /// Only applications in that stage. An unknown value is a `400`.
    pub status: Option<ApplicationStatus>,
    /// Only applications for that company. Not a UUID is a `400`.
    pub company_id: Option<Uuid>,
    /// Case-insensitive substring over the role title. At most 200 characters.
    #[param(example = "engineer")]
    pub search: Option<String>,
    /// Inclusive lower bound on `applied_on`, `YYYY-MM-DD`.
    #[param(example = "2026-10-01")]
    pub applied_from: Option<NaiveDate>,
    /// Inclusive upper bound on `applied_on`, `YYYY-MM-DD`.
    #[param(example = "2026-10-31")]
    pub applied_to: Option<NaiveDate>,
}

/// The whole query string of `GET /api/v1/job-applications`, deserialized once.
///
/// See [`CompaniesListQuery`] for why the fields are written out rather than
/// flattened.
#[derive(Debug, Clone, Deserialize)]
pub struct ApplicationsListQuery {
    /// Maximum number of applications to return.
    pub limit: Option<i64>,
    /// Number of applications to skip.
    pub offset: Option<i64>,
    /// Only applications in that stage.
    pub status: Option<ApplicationStatus>,
    /// Only applications for that company.
    pub company_id: Option<Uuid>,
    /// Case-insensitive substring over the role title.
    pub search: Option<String>,
    /// Inclusive lower bound on `applied_on`.
    pub applied_from: Option<NaiveDate>,
    /// Inclusive upper bound on `applied_on`.
    pub applied_to: Option<NaiveDate>,
}

impl ApplicationsListQuery {
    /// Splits the parsed query into its filter and pagination halves.
    pub fn into_parts(self) -> (ApplicationQuery, PaginationQuery) {
        let Self {
            limit,
            offset,
            status,
            company_id,
            search,
            applied_from,
            applied_to,
        } = self;

        (
            ApplicationQuery {
                status,
                company_id,
                search,
                applied_from,
                applied_to,
            },
            PaginationQuery { limit, offset },
        )
    }
}

/// Distinguishes `"field": null` from an absent key.
///
/// `#[serde(default)]` handles absence (`None`) and short-circuits this
/// function; when the key *is* present the function runs, and `null`
/// deserializes as `Option::<T>::deserialize(..) == None`, wrapped into
/// `Some(None)`. Drop `default` and the missing-field deserializer answers
/// `visit_none`, which puts absence back to `Some(None)`; drop this function
/// and `null` becomes `None`. Both are required.
fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Absence and an explicit `null` must stay apart, or a `PATCH` cannot tell
    /// "leave the website alone" from "delete it". Drop either `#[serde(default)]`
    /// or [`double_option`] and this test fails.
    #[test]
    fn a_nullable_company_field_is_kept_cleared_or_set_by_the_three_body_forms() {
        let kept: UpdateCompanyRequest =
            serde_json::from_str("{}").expect("an empty body is a valid patch");
        assert_eq!(kept.website_change(), (false, None));

        let cleared: UpdateCompanyRequest = serde_json::from_str(r#"{"website": null}"#)
            .expect("an explicit null is a valid patch");
        assert_eq!(cleared.website_change(), (true, None));

        let set: UpdateCompanyRequest =
            serde_json::from_str(r#"{"website":"https://acme.example.test"}"#)
                .expect("a string is a valid patch");
        assert_eq!(
            set.website_change(),
            (false, Some("https://acme.example.test"))
        );
    }

    /// The same three states on the application's nullable fields.
    #[test]
    fn a_nullable_application_field_is_kept_cleared_or_set_by_the_three_body_forms() {
        let kept: UpdateApplicationRequest =
            serde_json::from_str("{}").expect("an empty body is a valid patch");
        assert_eq!(kept.source_change(), (false, None));
        assert_eq!(kept.job_url_change(), (false, None));
        assert_eq!(kept.next_action_change(), (false, None));
        assert!(!kept.touches_salary());

        let cleared: UpdateApplicationRequest = serde_json::from_str(
            r#"{"source": null, "job_url": null, "next_action_task_id": null}"#,
        )
        .expect("an explicit null is a valid patch");
        assert_eq!(cleared.source_change(), (true, None));
        assert_eq!(cleared.job_url_change(), (true, None));
        assert_eq!(cleared.next_action_change(), (true, None));

        let id = Uuid::new_v4();
        let set: UpdateApplicationRequest = serde_json::from_str(&format!(
            r#"{{"source":"referral","job_url":"https://acme.example.test/42","next_action_task_id":"{id}"}}"#
        ))
        .expect("three values are a valid patch");
        assert_eq!(set.source_change(), (false, Some("referral")));
        assert_eq!(
            set.job_url_change(),
            (false, Some("https://acme.example.test/42"))
        );
        assert_eq!(set.next_action_change(), (false, Some(id)));

        // A salary field, even cleared, is a salary touch: the triple is
        // validated together, so the stored one has to be read.
        let salary: UpdateApplicationRequest =
            serde_json::from_str(r#"{"salary_min": null}"#).expect("null clears a bound");
        assert!(salary.touches_salary());
    }

    /// The event kind vocabulary is the database enum, so an unknown one never
    /// reaches the statement: the body fails to deserialize, which the shared
    /// extractor answers with `422`.
    #[test]
    fn an_event_kind_is_one_of_six() {
        for accepted in ["note", "email", "call", "interview", "offer", "rejection"] {
            let body = format!(
                r#"{{"kind":"{accepted}","occurred_at":"2026-10-14T13:00:00Z","summary":"x"}}"#
            );
            let parsed: Result<CreateEventRequest, _> = serde_json::from_str(&body);
            assert!(
                parsed.is_ok(),
                "{accepted:?} is one of the six published kinds and must parse"
            );
        }

        let refused = r#"{"kind":"meeting","occurred_at":"2026-10-14T13:00:00Z","summary":"x"}"#;
        assert!(
            serde_json::from_str::<CreateEventRequest>(refused).is_err(),
            "`meeting` is not one of the six kinds and must be refused"
        );
    }

    /// `occurred_at` is `DateTime<Utc>`, so a value without an offset has no
    /// unambiguous instant and is refused rather than guessed.
    #[test]
    fn an_event_occurred_at_requires_a_timezone() {
        let without_offset = r#"{"kind":"note","occurred_at":"2026-10-14T13:00:00","summary":"x"}"#;
        assert!(serde_json::from_str::<CreateEventRequest>(without_offset).is_err());

        let with_offset =
            r#"{"kind":"note","occurred_at":"2026-10-14T13:00:00+02:00","summary":"x"}"#;
        assert!(serde_json::from_str::<CreateEventRequest>(with_offset).is_ok());
    }

    /// `defaults to wishlist` is enforced by the service, not by the type: an
    /// omitted stage is `None` here, and the service refuses every stage except
    /// `wishlist`. This pins the parse, so the two halves cannot disagree.
    #[test]
    fn a_create_without_a_stage_parses_and_the_stage_is_absent() {
        let body = format!(
            r#"{{"company_id":"{}","role_title":"Backend engineer"}}"#,
            Uuid::nil()
        );
        let request: CreateApplicationRequest =
            serde_json::from_str(&body).expect("a create needs only a company and a role");
        assert!(request.status.is_none());
        assert_eq!(request.description, "");
    }
}
