//! Company, application and event persistence.
//!
//! Every statement is scoped by `user_id` in its own `WHERE` clause, never by a
//! check in the handler (`ADR-0005`). A row belonging to another user is
//! therefore invisible to the repository, which is what makes the `404`-not-
//! `403` guarantee enforceable in one place.
//!
//! # Money
//!
//! `salary_min` and `salary_max` are selected as `salary_min::text` /
//! `salary_max::text` and decode into [`String`]; see
//! [`crate::job_applications::models`]. The `AS` alias is not strictly required
//! — PostgreSQL names a cast expression after its column — but it makes the
//! `FromRow` mapping independent of that detail.
//!
//! # The two ownership links the schema cannot express
//!
//! * `next_action_task_id` has no composite key to reference, because `tasks`
//!   carries no `user_id`: its owner is inherited through `projects`. The
//!   predicate `EXISTS (SELECT 1 FROM tasks t JOIN projects p ON p.id =
//!   t.project_id WHERE t.id = $n AND p.user_id = $m)` is therefore written
//!   **inside** the write statements, so a foreign task is refused without a
//!   second round trip and without a check-then-act race.
//! * A company name is unique per owner only because of the functional index
//!   `companies_user_id_lower_name_unique_idx`. No `SELECT` can make that a
//!   guarantee, so a duplicate is detected by the `23505` the index raises and
//!   reported as a [`CompanyWrite::DuplicateName`] — never by a pre-check.
//!
//! # `updated_at` on an application
//!
//! A `PATCH` that only re-sends the current stage changes nothing, including
//! `updated_at`. The `CASE` in [`UPDATE_APPLICATION_OWNED`] is what makes that
//! true: every `SET` expression in an `UPDATE` reads the **old** row, so the
//! `status` the `updated_at` arm compares is the one before the statement.

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::job_applications::models::{
    ApplicationEvent, ApplicationEventKind, ApplicationStatus, Company, JobApplication,
};

/// The three salary columns, resolved.
///
/// A struct rather than three positional arguments, and one type for both
/// directions: the service resolves a request against the stored triple into
/// this shape, the write binds the three values from it, and the read that
/// resolves a `PATCH` returns it. The invariant the type does *not* carry by
/// itself — a bound requires a currency, and no currency means no bound — is
/// the service's, because it is the only layer that can name the field a client
/// has to correct.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SalaryRange {
    /// Four-decimal money string, never parsed in Rust.
    pub min: Option<String>,
    /// Four-decimal money string, never parsed in Rust.
    pub max: Option<String>,
    /// ISO 4217 code, upper case.
    pub currency: Option<String>,
}

/// The filters applied to a company listing.
///
/// A struct rather than positional arguments: an argument order that can be
/// silently swapped is a bug generator. The search term has already been
/// validated and normalised by [`crate::job_applications::service::JobService`].
#[derive(Debug, Clone, Copy, Default)]
pub struct CompanyListFilter<'a> {
    /// Substring of the name, already trimmed and non-empty.
    pub search: Option<&'a str>,
}

/// The filters applied to an application listing.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApplicationListFilter<'a> {
    /// Only applications in this stage.
    pub status: Option<ApplicationStatus>,
    /// Only applications for this company.
    pub company_id: Option<Uuid>,
    /// Substring of the role title, already trimmed and non-empty.
    pub search: Option<&'a str>,
    /// Inclusive lower bound on `applied_on`.
    pub applied_from: Option<NaiveDate>,
    /// Inclusive upper bound on `applied_on`.
    pub applied_to: Option<NaiveDate>,
}

/// One new company, as the service validated it.
#[derive(Debug, Clone, Copy)]
pub struct NewCompany<'a> {
    /// The freshly generated identifier.
    pub id: Uuid,
    /// The caller, from the credential and never from the body.
    pub owner: Uuid,
    /// Trimmed, 1 to 200 characters.
    pub name: &'a str,
    /// Absolute `http`/`https` reference, or `None`.
    pub website: Option<&'a str>,
    /// Free text, at most 5 000 characters.
    pub notes: &'a str,
}

/// The resolved tri-state of a company patch, as the statement binds it.
#[derive(Debug, Clone, Copy, Default)]
pub struct CompanyPatch<'a> {
    /// New name, or `None` to keep the stored one.
    pub name: Option<&'a str>,
    /// Whether the website is cleared.
    pub clear_website: bool,
    /// New website, or `None` to keep the stored one.
    pub website: Option<&'a str>,
    /// New notes, or `None` to keep the stored ones.
    pub notes: Option<&'a str>,
}

/// The outcome of a company insert or rename.
///
/// The repository never builds an error: the messages, and therefore the
/// statuses, are the service's. Each variant names exactly one thing the caller
/// has to translate, so a new outcome cannot be silently ignored.
#[derive(Debug)]
pub enum CompanyWrite<T> {
    /// The write applied.
    Applied(T),
    /// The functional unique index refused the name: the caller already has a
    /// company whose name equals this one ignoring case.
    DuplicateName,
}

/// One new application, as the service validated it.
#[derive(Debug, Clone, Copy)]
pub struct NewApplication<'a> {
    /// The freshly generated identifier.
    pub id: Uuid,
    /// The caller, from the credential and never from the body.
    pub owner: Uuid,
    /// The employer; must belong to the caller.
    pub company_id: Uuid,
    /// Trimmed role title, 1 to 200 characters.
    pub role_title: &'a str,
    /// Trimmed source label, or `None`.
    pub source: Option<&'a str>,
    /// The calendar day, or `None`.
    pub applied_on: Option<NaiveDate>,
    /// The resolved salary triple; consistent by construction.
    pub salary: &'a SalaryRange,
    /// Absolute `http`/`https` reference, or `None`.
    pub job_url: Option<&'a str>,
    /// Free text, at most 10 000 characters.
    pub description: &'a str,
    /// The caller's task, or `None`.
    pub next_action_task_id: Option<Uuid>,
}

/// The resolved patch of an application, as the statement binds it.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApplicationPatch<'a> {
    /// A corrected company, or `None` to keep the stored one.
    pub company_id: Option<Uuid>,
    /// New role title, or `None` to keep the stored one.
    pub role_title: Option<&'a str>,
    /// Whether the source label is cleared.
    pub clear_source: bool,
    /// New source label, or `None` to keep the stored one.
    pub source: Option<&'a str>,
    /// The requested stage, or `None` to leave the stage alone.
    pub status: Option<ApplicationStatus>,
    /// Whether `applied_on` is cleared.
    pub clear_applied_on: bool,
    /// New `applied_on`, or `None` to keep the stored one.
    pub applied_on: Option<NaiveDate>,
    /// The resolved salary triple, or `None` to leave all three columns alone.
    pub salary: Option<&'a SalaryRange>,
    /// Whether the job reference is cleared.
    pub clear_job_url: bool,
    /// New job reference, or `None` to keep the stored one.
    pub job_url: Option<&'a str>,
    /// New description, or `None` to keep the stored one.
    pub description: Option<&'a str>,
    /// Whether the next-action link is cleared.
    pub clear_next_action: bool,
    /// New next-action task, or `None` to keep the stored one.
    pub next_action_task_id: Option<Uuid>,
}

impl ApplicationPatch<'_> {
    /// Whether the request asks for any column other than `status` to be
    /// written.
    ///
    /// It decides whether `updated_at` is stamped: a `PATCH` whose only field
    /// is the stage the row already carries is the published no-op and must
    /// leave the row byte-identical.
    fn touches_row(&self) -> bool {
        self.company_id.is_some()
            || self.role_title.is_some()
            || self.clear_source
            || self.source.is_some()
            || self.clear_applied_on
            || self.applied_on.is_some()
            || self.salary.is_some()
            || self.clear_job_url
            || self.job_url.is_some()
            || self.description.is_some()
            || self.clear_next_action
            || self.next_action_task_id.is_some()
    }
}

/// One new event, as the service validated it.
#[derive(Debug, Clone, Copy)]
pub struct NewEvent<'a> {
    /// The freshly generated identifier.
    pub id: Uuid,
    /// The caller, from the credential and never from the body.
    pub owner: Uuid,
    /// The application the event belongs to; must belong to the caller.
    pub application_id: Uuid,
    /// The kind.
    pub kind: ApplicationEventKind,
    /// The instant on the timeline; may be in the future.
    pub occurred_at: DateTime<Utc>,
    /// Trimmed summary, 1 to 200 characters.
    pub summary: &'a str,
    /// Free text, at most 10 000 characters.
    pub details: &'a str,
}

/// The resolved patch of an event, as the statement binds it.
///
/// There is no `updated_at` field and no timestamp column in the statement: an
/// event is corrected in place (`A-D1-1`).
#[derive(Debug, Clone, Copy, Default)]
pub struct EventPatch<'a> {
    /// New kind, or `None` to keep the stored one.
    pub kind: Option<ApplicationEventKind>,
    /// New instant, or `None` to keep the stored one.
    pub occurred_at: Option<DateTime<Utc>>,
    /// New summary, or `None` to keep the stored one.
    pub summary: Option<&'a str>,
    /// New details, or `None` to keep the stored one.
    pub details: Option<&'a str>,
}

/// Creates a company.
///
/// A duplicate name is not checked here: the functional unique index
/// `companies_user_id_lower_name_unique_idx` is the guarantee, and it is
/// detected as a `23505` by [`JobRepository::insert_company`]. The stored
/// casing is whatever the service trimmed and passed, because only the
/// *comparison* is folded.
const INSERT_COMPANY: &str = r#"
    INSERT INTO companies (id, user_id, name, website, notes)
    VALUES ($1, $2, $3, $4, $5)
    RETURNING id, user_id, name, website, notes, created_at, updated_at
"#;

/// Reads one company of the caller.
const FIND_COMPANY_OWNED: &str = r#"
    SELECT id, user_id, name, website, notes, created_at, updated_at
    FROM companies
    WHERE id = $1 AND user_id = $2
"#;

/// The page statement for the company listing.
///
/// `companies_user_id_lower_name_name_id_idx` supplies the order; the `id`
/// tiebreaker makes it total, which the index alone would not.
const PAGE_COMPANIES: &str = r#"
    SELECT id, user_id, name, website, notes, created_at, updated_at
    FROM companies
    WHERE user_id = $1
      AND ($2::text IS NULL OR name ILIKE '%' || $2 || '%')
    ORDER BY LOWER(name), name, id
    LIMIT $3 OFFSET $4
"#;

/// The count statement matching [`PAGE_COMPANIES`].
const COUNT_COMPANIES: &str = r#"
    SELECT COUNT(*)
    FROM companies
    WHERE user_id = $1
      AND ($2::text IS NULL OR name ILIKE '%' || $2 || '%')
"#;

/// Applies a partial update to a company.
///
/// `COALESCE($n, column)` is the partial-update rule made mechanical: `NULL` in
/// the binds means "keep". The boolean is needed for the website because a
/// `NULL` website already means "leave alone"; only the boolean can say
/// "clear".
const UPDATE_COMPANY_OWNED: &str = r#"
    UPDATE companies
    SET name       = COALESCE($3, name),
        website    = CASE WHEN $4 THEN NULL ELSE COALESCE($5, website) END,
        notes      = COALESCE($6, notes),
        updated_at = clock_timestamp()
    WHERE id = $1 AND user_id = $2
    RETURNING id, user_id, name, website, notes, created_at, updated_at
"#;

/// Deletes a company **only** when no application references it.
///
/// One statement, not a `SELECT COUNT(*)` followed by a `DELETE`: an
/// application can be inserted between the two, and that race is exactly what
/// this rule exists to prevent. Zero rows is therefore ambiguous — the company
/// is absent, foreign, or still in use — and the service resolves it on its
/// error path with one owner-scoped read.
///
/// The applications are never deleted to make this succeed: they are records
/// the client did not ask to delete.
const DELETE_COMPANY_IF_UNUSED: &str = r#"
    DELETE FROM companies c
    WHERE c.id = $1
      AND c.user_id = $2
      AND NOT EXISTS (
          SELECT 1 FROM job_applications a
          WHERE a.company_id = $1 AND a.user_id = $2
      )
"#;

/// Creates an application.
///
/// Both ownership links are predicates of this statement, so neither costs a
/// second round trip:
///
/// * the company, through the composite key's parent — `c.id = $3 AND c.user_id
///   = $2` — which also satisfies `job_applications_company_fk`;
/// * the optional task, through `tasks → projects → user_id`, because `tasks`
///   has no `user_id` column for a composite key to reference.
///
/// Zero rows therefore means "the company is not the caller's, or the task is
/// not"; the service names which on its error path.
const INSERT_APPLICATION: &str = r#"
    INSERT INTO job_applications
        (id, user_id, company_id, role_title, source, status, applied_on,
         salary_min, salary_max, salary_currency, job_url, description,
         next_action_task_id)
    SELECT $1, $2, c.id, $4, $5, 'wishlist'::job_application_status, $6::date,
           $7::numeric, $8::numeric, $9::char(3), $10, $11, $12::uuid
    FROM companies c
    WHERE c.id = $3
      AND c.user_id = $2
      AND ($12::uuid IS NULL
           OR EXISTS (SELECT 1 FROM tasks t
                      JOIN projects p ON p.id = t.project_id
                      WHERE t.id = $12::uuid AND p.user_id = $2))
    RETURNING id, user_id, company_id, role_title, source, status, applied_on,
              salary_min::text AS salary_min, salary_max::text AS salary_max,
              salary_currency, job_url, description, next_action_task_id,
              created_at, updated_at
"#;

/// Reads one application of the caller.
const FIND_APPLICATION_OWNED: &str = r#"
    SELECT id, user_id, company_id, role_title, source, status, applied_on,
           salary_min::text AS salary_min, salary_max::text AS salary_max,
           salary_currency, job_url, description, next_action_task_id,
           created_at, updated_at
    FROM job_applications
    WHERE id = $1 AND user_id = $2
"#;

/// Reads the salary triple of one application of the caller.
///
/// Used only to resolve a `PATCH` that touches a salary column: the three
/// columns are validated together, so "clear the minimum" has to know whether a
/// maximum and a currency are still there. It never decides whether the row
/// exists — a miss leaves the resolution undefined and the guarded `UPDATE`
/// answers `404`.
const FIND_SALARY_OWNED: &str = r#"
    SELECT salary_min::text AS salary_min, salary_max::text AS salary_max,
           salary_currency
    FROM job_applications
    WHERE id = $1 AND user_id = $2
"#;

/// The page statement for the application listing.
///
/// `job_applications_user_id_created_at_idx` serves the unfiltered shape;
/// `job_applications_user_id_status_created_at_idx` is the board view. Unlike
/// the claim listing, this one is **not** split into a filtered and an
/// unfiltered statement: three more filters (company, role, two dates) mean no
/// single listing index could serve the filtered shape anyway, so a second copy
/// of the statement would buy nothing. See the module docs of
/// [`crate::job_applications`] for the decision.
///
/// The date bounds compare against `applied_on` directly: it is a `DATE`, not
/// an instant, so an inclusive comparison needs no `AT TIME ZONE` and cannot
/// depend on the session's zone. A `NULL` `applied_on` makes both comparisons
/// `NULL`, which drops the row — the published behaviour when either bound is
/// given.
const PAGE_APPLICATIONS: &str = r#"
    SELECT id, user_id, company_id, role_title, source, status, applied_on,
           salary_min::text AS salary_min, salary_max::text AS salary_max,
           salary_currency, job_url, description, next_action_task_id,
           created_at, updated_at
    FROM job_applications
    WHERE user_id = $1
      AND ($2::job_application_status IS NULL OR status = $2::job_application_status)
      AND ($3::uuid IS NULL OR company_id = $3::uuid)
      AND ($4::text IS NULL OR role_title ILIKE '%' || $4 || '%')
      AND ($5::date IS NULL OR applied_on >= $5::date)
      AND ($6::date IS NULL OR applied_on <= $6::date)
    ORDER BY created_at DESC, id DESC
    LIMIT $7 OFFSET $8
"#;

/// The count statement matching [`PAGE_APPLICATIONS`].
const COUNT_APPLICATIONS: &str = r#"
    SELECT COUNT(*)
    FROM job_applications
    WHERE user_id = $1
      AND ($2::job_application_status IS NULL OR status = $2::job_application_status)
      AND ($3::uuid IS NULL OR company_id = $3::uuid)
      AND ($4::text IS NULL OR role_title ILIKE '%' || $4 || '%')
      AND ($5::date IS NULL OR applied_on >= $5::date)
      AND ($6::date IS NULL OR applied_on <= $6::date)
"#;

/// Applies a partial update to an application, moving its stage when asked.
///
/// Binds are `id`, `owner`, `company_id`, `role_title`, `clear_source`,
/// `source`, `status`, `clear_applied_on`, `applied_on`, `salary present`,
/// `salary_min`, `salary_max`, `salary_currency`, `clear_job_url`, `job_url`,
/// `description`, `clear_next_action`, `next_action_task_id`, `touch`,
/// `permitted sources` — exactly the twenty values [`ApplicationPatch`] and the
/// transition table resolve to.
///
/// # The stage guard
///
/// The requested stage is written only when the current one is either the
/// requested stage itself (the published **no-op**) or one of the stages the
/// machine allows to reach it. Both are one `WHERE` predicate over the array
/// the service derives from the same table [`crate::job_applications::service::
/// ensure_transition`] reads, so the two cannot disagree.
///
/// Zero rows is therefore ambiguous — the row is absent or foreign, the move is
/// refused, the company is foreign, or the task is foreign — and the service
/// resolves it on its **error path**. There is deliberately no read before this
/// statement: a read-then-write would let two requests both see the same stage
/// and both win.
///
/// # The salary triple
///
/// The three columns are written together when the request touches any of them,
/// from the triple the service resolved and validated against the stored one.
/// Writing the resolved triple rather than a `COALESCE` per column is what keeps
/// a concurrent salary edit from producing a combination the `CHECK` constraints
/// would refuse: whatever this statement writes was validated whole, so the
/// client sees a `422` naming the field or a committed consistent row, never a
/// `23514`.
const UPDATE_APPLICATION_OWNED: &str = r#"
    UPDATE job_applications
    SET company_id          = COALESCE($3::uuid, company_id),
        role_title          = COALESCE($4, role_title),
        source              = CASE WHEN $5 THEN NULL ELSE COALESCE($6, source) END,
        status              = CASE WHEN $7::job_application_status IS NULL
                                        OR status = $7::job_application_status
                                   THEN status
                                   ELSE $7::job_application_status END,
        applied_on          = CASE WHEN $8 THEN NULL ELSE COALESCE($9::date, applied_on) END,
        salary_min          = CASE WHEN $10 THEN $11::numeric ELSE salary_min END,
        salary_max          = CASE WHEN $10 THEN $12::numeric ELSE salary_max END,
        salary_currency     = CASE WHEN $10 THEN $13::char(3) ELSE salary_currency END,
        job_url             = CASE WHEN $14 THEN NULL ELSE COALESCE($15, job_url) END,
        description         = COALESCE($16, description),
        next_action_task_id = CASE WHEN $17 THEN NULL
                                   ELSE COALESCE($18::uuid, next_action_task_id) END,
        updated_at          = CASE WHEN $19
                                        OR ($7::job_application_status IS NOT NULL
                                            AND status <> $7::job_application_status)
                                   THEN clock_timestamp()
                                   ELSE updated_at END
    WHERE id = $1
      AND user_id = $2
      AND ($7::job_application_status IS NULL
           OR status = ANY($20::job_application_status[]))
      AND ($3::uuid IS NULL
           OR EXISTS (SELECT 1 FROM companies c WHERE c.id = $3::uuid AND c.user_id = $2))
      AND ($18::uuid IS NULL
           OR EXISTS (SELECT 1 FROM tasks t
                      JOIN projects p ON p.id = t.project_id
                      WHERE t.id = $18::uuid AND p.user_id = $2))
    RETURNING id, user_id, company_id, role_title, source, status, applied_on,
              salary_min::text AS salary_min, salary_max::text AS salary_max,
              salary_currency, job_url, description, next_action_task_id,
              created_at, updated_at
"#;

/// Deletes one application of the caller.
///
/// Its events go with it through `application_events_application_fk … ON DELETE
/// CASCADE`, in the same statement, so no transaction is needed.
const DELETE_APPLICATION_OWNED: &str =
    "DELETE FROM job_applications WHERE id = $1 AND user_id = $2";

/// The page statement for the event listing of one application.
///
/// `application_events_application_id_occurred_at_idx` supplies the leading
/// order; `id DESC` is the total tiebreaker the contract requires for two
/// entries at the same instant.
const PAGE_EVENTS: &str = r#"
    SELECT id, user_id, application_id, kind, occurred_at, summary, details, created_at
    FROM application_events
    WHERE application_id = $1 AND user_id = $2
    ORDER BY occurred_at DESC, id DESC
    LIMIT $3 OFFSET $4
"#;

/// The count statement matching [`PAGE_EVENTS`].
const COUNT_EVENTS: &str = r#"
    SELECT COUNT(*)
    FROM application_events
    WHERE application_id = $1 AND user_id = $2
"#;

/// Whether an application of the caller exists, and how many events it has.
///
/// One statement, asked only when the page came back empty: an empty page has
/// two different meanings — the application is absent or foreign (`404`), or
/// the page is simply empty (an application with no events, or an offset past
/// the end) — and the reported `total` must be the size of the filtered set
/// either way, not the size of the page.
const EVENTS_EXISTS_WITH_TOTAL: &str = r#"
    SELECT
        EXISTS (SELECT 1 FROM job_applications WHERE id = $1 AND user_id = $2),
        (SELECT COUNT(*)
         FROM application_events
         WHERE application_id = $1 AND user_id = $2)
"#;

/// Inserts one event of the caller's application.
///
/// `user_id` is written from the caller and never from the body, and the
/// `SELECT … FROM job_applications` is the ownership proof: `application_events
/// _application_fk` independently refuses a row whose owner is not its
/// application's owner, but that would be a `23503`, and the contract publishes
/// `404`.
const INSERT_EVENT: &str = r#"
    INSERT INTO application_events
        (id, user_id, application_id, kind, occurred_at, summary, details)
    SELECT $1, $2, a.id, $4::application_event_kind, $5, $6, $7
    FROM job_applications a
    WHERE a.id = $3 AND a.user_id = $2
    RETURNING id, user_id, application_id, kind, occurred_at, summary, details, created_at
"#;

/// Applies a partial update to one event, in place.
///
/// Both identifiers are bound, so an event id that exists under another
/// application of the same owner matches no row. No timestamp is written: the
/// table has no `updated_at` column and the response has no such field
/// (`A-D1-1`).
const UPDATE_EVENT_OWNED: &str = r#"
    UPDATE application_events
    SET kind        = COALESCE($4::application_event_kind, kind),
        occurred_at = COALESCE($5, occurred_at),
        summary     = COALESCE($6, summary),
        details     = COALESCE($7, details)
    WHERE id = $1 AND application_id = $2 AND user_id = $3
    RETURNING id, user_id, application_id, kind, occurred_at, summary, details, created_at
"#;

/// Deletes one event, inside its application.
const DELETE_EVENT_OWNED: &str =
    "DELETE FROM application_events WHERE id = $1 AND application_id = $2 AND user_id = $3";

/// Whether a company of that id belongs to the caller.
const COMPANY_IS_OWNED: &str =
    "SELECT EXISTS (SELECT 1 FROM companies WHERE id = $1 AND user_id = $2)";

/// Whether a task of that id belongs to the caller, through its project.
const TASK_IS_OWNED: &str = r#"
    SELECT EXISTS (
        SELECT 1 FROM tasks t
        JOIN projects p ON p.id = t.project_id
        WHERE t.id = $1 AND p.user_id = $2
    )
"#;

/// Persistence operations for companies, applications and events.
#[derive(Debug, Clone)]
pub struct JobRepository {
    pool: PgPool,
}

impl JobRepository {
    /// Creates a repository bound to a connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Inserts a company.
    ///
    /// A `23505` naming `companies_user_id_lower_name_unique_idx` is
    /// [`CompanyWrite::DuplicateName`], not a database error: the index is the
    /// rule, and the race a pre-check would leave open is exactly the case it
    /// catches. Every other database error is reported as it is, so it reaches
    /// the client as the redacted `500` it is.
    pub async fn insert_company(
        &self,
        company: &NewCompany<'_>,
    ) -> AppResult<CompanyWrite<Company>> {
        let result = sqlx::query_as::<_, Company>(INSERT_COMPANY)
            .bind(company.id)
            .bind(company.owner)
            .bind(company.name)
            .bind(company.website)
            .bind(company.notes)
            .fetch_optional(&self.pool)
            .await;

        match result {
            Ok(Some(company)) => Ok(CompanyWrite::Applied(company)),
            Ok(None) => Err(AppError::Internal(
                "the company insert returned no row".to_string(),
            )),
            Err(error) if is_duplicate_company_name(&error) => Ok(CompanyWrite::DuplicateName),
            Err(error) => Err(AppError::Database(error)),
        }
    }

    /// Finds a company owned by `owner`.
    pub async fn find_company_owned(
        &self,
        company_id: Uuid,
        owner: Uuid,
    ) -> AppResult<Option<Company>> {
        let company = sqlx::query_as::<_, Company>(FIND_COMPANY_OWNED)
            .bind(company_id)
            .bind(owner)
            .fetch_optional(&self.pool)
            .await?;

        Ok(company)
    }

    /// Lists the companies owned by `owner`, with the total of the filtered set.
    pub async fn list_companies_owned(
        &self,
        owner: Uuid,
        filter: &CompanyListFilter<'_>,
        limit: i64,
        offset: i64,
    ) -> AppResult<(Vec<Company>, i64)> {
        let companies = sqlx::query_as::<_, Company>(PAGE_COMPANIES)
            .bind(owner)
            .bind(filter.search)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        let total = sqlx::query_scalar::<_, i64>(COUNT_COMPANIES)
            .bind(owner)
            .bind(filter.search)
            .fetch_one(&self.pool)
            .await?;

        Ok((companies, total))
    }

    /// Applies a partial update to a company.
    ///
    /// `None` means the company is absent or foreign. A rename onto another
    /// company's name is [`CompanyWrite::DuplicateName`], for the same reason as
    /// the insert.
    pub async fn update_company_owned(
        &self,
        company_id: Uuid,
        owner: Uuid,
        patch: &CompanyPatch<'_>,
    ) -> AppResult<CompanyWrite<Option<Company>>> {
        let result = sqlx::query_as::<_, Company>(UPDATE_COMPANY_OWNED)
            .bind(company_id)
            .bind(owner)
            .bind(patch.name)
            .bind(patch.clear_website)
            .bind(patch.website)
            .bind(patch.notes)
            .fetch_optional(&self.pool)
            .await;

        match result {
            Ok(company) => Ok(CompanyWrite::Applied(company)),
            Err(error) if is_duplicate_company_name(&error) => Ok(CompanyWrite::DuplicateName),
            Err(error) => Err(AppError::Database(error)),
        }
    }

    /// Deletes a company that no application references.
    ///
    /// `false` means the statement refused: the company is absent, foreign, or
    /// still in use. The three cases are told apart by the service on its error
    /// path, because only one of them is a `404`.
    pub async fn delete_company_if_unused(&self, company_id: Uuid, owner: Uuid) -> AppResult<bool> {
        let result = sqlx::query(DELETE_COMPANY_IF_UNUSED)
            .bind(company_id)
            .bind(owner)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Inserts an application.
    ///
    /// Returns `None` when the company is not the caller's, or the optional task
    /// is not; the service names which on its error path.
    pub async fn insert_application(
        &self,
        application: &NewApplication<'_>,
    ) -> AppResult<Option<JobApplication>> {
        let salary = application.salary;

        let inserted = sqlx::query_as::<_, JobApplication>(INSERT_APPLICATION)
            .bind(application.id)
            .bind(application.owner)
            .bind(application.company_id)
            .bind(application.role_title)
            .bind(application.source)
            .bind(application.applied_on)
            .bind(salary.min.as_deref())
            .bind(salary.max.as_deref())
            .bind(salary.currency.as_deref())
            .bind(application.job_url)
            .bind(application.description)
            .bind(application.next_action_task_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(inserted)
    }

    /// Finds an application owned by `owner`.
    pub async fn find_application_owned(
        &self,
        application_id: Uuid,
        owner: Uuid,
    ) -> AppResult<Option<JobApplication>> {
        let application = sqlx::query_as::<_, JobApplication>(FIND_APPLICATION_OWNED)
            .bind(application_id)
            .bind(owner)
            .fetch_optional(&self.pool)
            .await?;

        Ok(application)
    }

    /// Reads the stored salary triple of one application of the caller.
    ///
    /// `None` means the application is absent or foreign, which the guarded
    /// write below answers as a `404`; this read never decides it.
    pub async fn find_salary_owned(
        &self,
        application_id: Uuid,
        owner: Uuid,
    ) -> AppResult<Option<SalaryRange>> {
        let row = sqlx::query_as::<_, SalaryRange>(FIND_SALARY_OWNED)
            .bind(application_id)
            .bind(owner)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row)
    }

    /// Lists the applications owned by `owner`, with the total of the filtered
    /// set.
    pub async fn list_applications_owned(
        &self,
        owner: Uuid,
        filter: &ApplicationListFilter<'_>,
        limit: i64,
        offset: i64,
    ) -> AppResult<(Vec<JobApplication>, i64)> {
        let applications = sqlx::query_as::<_, JobApplication>(PAGE_APPLICATIONS)
            .bind(owner)
            .bind(filter.status)
            .bind(filter.company_id)
            .bind(filter.search)
            .bind(filter.applied_from)
            .bind(filter.applied_to)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        let total = sqlx::query_scalar::<_, i64>(COUNT_APPLICATIONS)
            .bind(owner)
            .bind(filter.status)
            .bind(filter.company_id)
            .bind(filter.search)
            .bind(filter.applied_from)
            .bind(filter.applied_to)
            .fetch_one(&self.pool)
            .await?;

        Ok((applications, total))
    }

    /// Applies a partial update to an application, moving its stage when the
    /// patch names one.
    ///
    /// `sources` is the set of stages the requested one may be reached from,
    /// including the requested stage itself for the no-op; the service derives
    /// it from [`crate::job_applications::service::permitted_sources`].
    ///
    /// `None` means the statement refused for one of four reasons — see
    /// [`UPDATE_APPLICATION_OWNED`] — and the service explains it on its error
    /// path.
    pub async fn update_application_owned(
        &self,
        application_id: Uuid,
        owner: Uuid,
        patch: &ApplicationPatch<'_>,
        sources: Option<Vec<ApplicationStatus>>,
    ) -> AppResult<Option<JobApplication>> {
        let salary = patch.salary;
        let touch = patch.touches_row();

        let updated = sqlx::query_as::<_, JobApplication>(UPDATE_APPLICATION_OWNED)
            .bind(application_id)
            .bind(owner)
            .bind(patch.company_id)
            .bind(patch.role_title)
            .bind(patch.clear_source)
            .bind(patch.source)
            .bind(patch.status)
            .bind(patch.clear_applied_on)
            .bind(patch.applied_on)
            .bind(salary.is_some())
            .bind(salary.and_then(|range| range.min.as_deref()))
            .bind(salary.and_then(|range| range.max.as_deref()))
            .bind(salary.and_then(|range| range.currency.as_deref()))
            .bind(patch.clear_job_url)
            .bind(patch.job_url)
            .bind(patch.description)
            .bind(patch.clear_next_action)
            .bind(patch.next_action_task_id)
            .bind(touch)
            .bind(sources)
            .fetch_optional(&self.pool)
            .await?;

        Ok(updated)
    }

    /// Deletes an application owned by `owner`. Its events cascade.
    pub async fn delete_application_owned(
        &self,
        application_id: Uuid,
        owner: Uuid,
    ) -> AppResult<bool> {
        let result = sqlx::query(DELETE_APPLICATION_OWNED)
            .bind(application_id)
            .bind(owner)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Lists the events of one application of the caller.
    ///
    /// `None` means the application is absent or foreign. The distinction
    /// between "no such application" and "an application with no events" is
    /// observable — one is a `404`, the other an empty page — so it is
    /// preserved. An empty page costs one extra statement that reports both the
    /// existence and the real total, so an offset past the end still reports the
    /// size of the filtered set; a page with rows pays for the count alone.
    pub async fn list_events_owned(
        &self,
        application_id: Uuid,
        owner: Uuid,
        limit: i64,
        offset: i64,
    ) -> AppResult<Option<(Vec<ApplicationEvent>, i64)>> {
        let events = sqlx::query_as::<_, ApplicationEvent>(PAGE_EVENTS)
            .bind(application_id)
            .bind(owner)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        if events.is_empty() {
            let (exists, total) = sqlx::query_as::<_, (bool, i64)>(EVENTS_EXISTS_WITH_TOTAL)
                .bind(application_id)
                .bind(owner)
                .fetch_one(&self.pool)
                .await?;

            return Ok(exists.then_some((events, total)));
        }

        let total = sqlx::query_scalar::<_, i64>(COUNT_EVENTS)
            .bind(application_id)
            .bind(owner)
            .fetch_one(&self.pool)
            .await?;

        Ok(Some((events, total)))
    }

    /// Inserts one event of the caller's application.
    ///
    /// `None` means the application is absent or foreign.
    pub async fn insert_event(&self, event: &NewEvent<'_>) -> AppResult<Option<ApplicationEvent>> {
        let inserted = sqlx::query_as::<_, ApplicationEvent>(INSERT_EVENT)
            .bind(event.id)
            .bind(event.owner)
            .bind(event.application_id)
            .bind(event.kind)
            .bind(event.occurred_at)
            .bind(event.summary)
            .bind(event.details)
            .fetch_optional(&self.pool)
            .await?;

        Ok(inserted)
    }

    /// Applies a partial update to one event, inside its application.
    ///
    /// `None` means the event is absent, foreign, or belongs to another
    /// application — the contract publishes one `404` for all three.
    pub async fn update_event_owned(
        &self,
        application_id: Uuid,
        event_id: Uuid,
        owner: Uuid,
        patch: &EventPatch<'_>,
    ) -> AppResult<Option<ApplicationEvent>> {
        let updated = sqlx::query_as::<_, ApplicationEvent>(UPDATE_EVENT_OWNED)
            .bind(event_id)
            .bind(application_id)
            .bind(owner)
            .bind(patch.kind)
            .bind(patch.occurred_at)
            .bind(patch.summary)
            .bind(patch.details)
            .fetch_optional(&self.pool)
            .await?;

        Ok(updated)
    }

    /// Deletes one event, inside its application.
    pub async fn delete_event_owned(
        &self,
        application_id: Uuid,
        event_id: Uuid,
        owner: Uuid,
    ) -> AppResult<bool> {
        let result = sqlx::query(DELETE_EVENT_OWNED)
            .bind(event_id)
            .bind(application_id)
            .bind(owner)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Whether a company belongs to the caller.
    ///
    /// Used only on an error path, to name which link of a refused write was
    /// the problem.
    pub async fn company_is_owned(&self, company_id: Uuid, owner: Uuid) -> AppResult<bool> {
        let owned = sqlx::query_scalar::<_, bool>(COMPANY_IS_OWNED)
            .bind(company_id)
            .bind(owner)
            .fetch_one(&self.pool)
            .await?;

        Ok(owned)
    }

    /// Whether a task belongs to the caller, through its project.
    ///
    /// This is the one ownership link in the feature that the database cannot
    /// enforce: `tasks` has no `user_id`, so there is no `(id, user_id)` pair
    /// for a composite key to reference. The check inside the write statements
    /// is the authority; this read exists to name the `404`.
    pub async fn task_is_owned(&self, task_id: Uuid, owner: Uuid) -> AppResult<bool> {
        let owned = sqlx::query_scalar::<_, bool>(TASK_IS_OWNED)
            .bind(task_id)
            .bind(owner)
            .fetch_one(&self.pool)
            .await?;

        Ok(owned)
    }
}

/// Whether a database error is the company-name unique index.
///
/// `23505` alone is not enough: `users_email_unique_idx` reaches the same code,
/// and reporting one of those as a company collision would be a lie a client
/// acts on. The constraint name is what makes the identification exact.
fn is_duplicate_company_name(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .filter(|database| database.code().as_deref() == Some("23505"))
        .and_then(|database| database.constraint())
        .is_some_and(|constraint| constraint == "companies_user_id_lower_name_unique_idx")
}

/// Reads the three salary columns into [`SalaryRange`].
///
/// `FromRow` is implemented by hand because the struct is not a row shape: its
/// three fields are a triple the service resolves, and the derive would give it
/// a `FromRow` that reads a table that does not exist.
impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for SalaryRange {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;

        Ok(Self {
            min: row.try_get("salary_min")?,
            max: row.try_get("salary_max")?,
            currency: row.try_get("salary_currency")?,
        })
    }
}
