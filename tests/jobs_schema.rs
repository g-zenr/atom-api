//! Integration tests for the job-application schema (`migrations/0005`).
//!
//! The chain `C-J1` adds three tables, two enum types and no endpoint, so there
//! is no router to drive: these tests speak SQL to the real, disposable
//! database. That is the only way to prove a constraint, because a `CHECK`, a
//! composite foreign key, an `ON DELETE` action and a functional unique index
//! are properties of PostgreSQL, not of this crate.
//!
//! Every rejection asserts the SQLSTATE and, where the constraint is the
//! subject, the constraint name the server reports. "An error happened" is not
//! evidence that the constraint under test is the thing that fired. Every
//! rejection is paired with an accepted control row, so a test cannot pass
//! against a table that refuses everything.
//!
//! The tests are independent. Each fixture is built from a fresh UUID and a
//! unique email address, and each assertion is restricted to the ids the test
//! created, so no test depends on the order another one ran in and none
//! truncates a shared table. The one shared resource is the disposable test
//! database itself, which the harness refuses to use unless its name contains
//! `test`.
//!
//! The contract is `C-J1` in `docs/engineering/runs/archive/phase-2/ledger.md`; the
//! constraints are defined in `docs/database/schema.md` §11.3, §11.4 and §11.5.

pub mod common;

use std::str::FromStr;

use chrono::{DateTime, TimeZone, Utc};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};
use uuid::Uuid;

/// The role title every fixture starts from. Length is a separate constraint
/// with its own test in a later chain; here it only has to be valid.
const ROLE_TITLE: &str = "a job application";

/// The seven stages the enum admits, in the order §11.8 lists them.
const STAGES: [&str; 7] = [
    "wishlist",
    "applied",
    "screening",
    "interview",
    "offer",
    "rejected",
    "withdrawn",
];

/// The base instant every fixture timestamp is derived from.
///
/// The value is fixed rather than read from the clock: a test may not depend on
/// a timing value, and `occurred_at` only has to be a valid `TIMESTAMPTZ`.
fn base_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 1, 9, 0, 0)
        .single()
        .expect("the fixture timestamp must be valid")
}

/// A pool on the disposable test database that carries the embedded migrations.
///
/// A test that writes a fixture row needs the schema whether or not another
/// test binary has already migrated this database in this process.
async fn migrated_pool() -> PgPool {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::test_database_url())
        .await
        .expect("the test database must be reachable");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("the migrations must apply to the test database");

    pool
}

/// Inserts an account and returns its id.
async fn insert_account(pool: &PgPool, prefix: &str) -> Uuid {
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO users (id, email, password_hash, display_name) VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(common::unique_email(prefix))
    .bind("jobs-schema-fixture-not-a-real-hash")
    .bind("Jobs Schema Fixture")
    .execute(pool)
    .await
    .expect("the fixture account must be inserted");

    id
}

/// Inserts a company and returns its id, or the error the server returned.
///
/// The name is a parameter rather than a constant: the trim and uniqueness
/// tests both need to pass a value the table must reject.
async fn insert_company(pool: &PgPool, user_id: Uuid, name: &str) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();

    sqlx::query("INSERT INTO companies (id, user_id, name) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(user_id)
        .bind(name)
        .execute(pool)
        .await?;

    Ok(id)
}

/// One `job_applications` row as a test writes it.
///
/// `status` is the wire token and the insert casts it to `job_application_status`,
/// so a misspelt stage fails at the server rather than being stored as text.
/// The two money columns are carried as decimal strings because SQLx cannot
/// decode `NUMERIC` without a decimal crate (ADR-0007), and the same text form
/// is what the API publishes. `applied_on` is cast from text for the same
/// reason: the fixture only needs a value the `DATE` column accepts.
struct NewApplication {
    user_id: Uuid,
    company_id: Uuid,
    role_title: &'static str,
    status: &'static str,
    applied_on: Option<&'static str>,
    salary_min: Option<&'static str>,
    salary_max: Option<&'static str>,
    salary_currency: Option<&'static str>,
    next_action_task_id: Option<Uuid>,
}

impl NewApplication {
    /// A `wishlist` application with no salary, no date and no next action: the
    /// only shape the constraints accept without further information.
    fn new(user_id: Uuid, company_id: Uuid) -> Self {
        Self {
            user_id,
            company_id,
            role_title: ROLE_TITLE,
            status: "wishlist",
            applied_on: None,
            salary_min: None,
            salary_max: None,
            salary_currency: None,
            next_action_task_id: None,
        }
    }
}

/// Inserts an application and returns its id, or the error the server returned.
async fn insert_application(
    pool: &PgPool,
    application: &NewApplication,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO job_applications
            (id, user_id, company_id, role_title, status, applied_on,
             salary_min, salary_max, salary_currency, next_action_task_id)
         VALUES ($1, $2, $3, $4, $5::text::job_application_status, $6::text::date,
                 $7::text::numeric, $8::text::numeric, $9, $10)",
    )
    .bind(id)
    .bind(application.user_id)
    .bind(application.company_id)
    .bind(application.role_title)
    .bind(application.status)
    .bind(application.applied_on)
    .bind(application.salary_min)
    .bind(application.salary_max)
    .bind(application.salary_currency)
    .bind(application.next_action_task_id)
    .execute(pool)
    .await?;

    Ok(id)
}

/// Inserts an event on an application and returns its id, or the error the
/// server returned.
///
/// `user_id` is a parameter rather than being read from the application: the
/// composite key exists so that a caller can pass a value that disagrees with
/// the application's owner, and the database has to reject it.
async fn insert_event(
    pool: &PgPool,
    user_id: Uuid,
    application_id: Uuid,
    kind: &'static str,
    day_offset: i64,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    let occurred_at = base_time() + chrono::Duration::days(day_offset);

    sqlx::query(
        "INSERT INTO application_events
            (id, user_id, application_id, kind, occurred_at, summary)
         VALUES ($1, $2, $3, $4::text::application_event_kind, $5, $6)",
    )
    .bind(id)
    .bind(user_id)
    .bind(application_id)
    .bind(kind)
    .bind(occurred_at)
    .bind("a fixture timeline entry")
    .execute(pool)
    .await?;

    Ok(id)
}

/// The SQLSTATE of a database error and the constraint it names, when it names
/// one.
fn violation_of(error: &sqlx::Error) -> (String, Option<String>) {
    match error {
        sqlx::Error::Database(database) => (
            database
                .code()
                .map(|code| code.into_owned())
                .unwrap_or_default(),
            database.constraint().map(str::to_string),
        ),
        other => panic!("expected a database error, got {other:?}"),
    }
}

/// Asserts that the statement failed with `sqlstate`, and returns the
/// constraint the server named.
fn rejected(result: Result<(), sqlx::Error>, sqlstate: &str) -> Option<String> {
    let error = match result {
        Ok(()) => panic!("the statement was accepted; it must be rejected with {sqlstate}"),
        Err(error) => error,
    };

    let (code, constraint) = violation_of(&error);
    assert_eq!(
        code, sqlstate,
        "unexpected SQLSTATE; the server said: {error}"
    );

    constraint
}

/// Asserts that the statement was rejected by a named constraint.
fn assert_rejected(result: Result<(), sqlx::Error>, sqlstate: &str, constraint: &str) {
    let reported = rejected(result, sqlstate);
    assert_eq!(
        reported.as_deref(),
        Some(constraint),
        "the violation must name {constraint}"
    );
}

/// A database this test created, dropped when it leaves scope.
///
/// `Drop` is a backstop, not the primary cleanup: the test removes the database
/// explicitly and checks that it is gone, and this guard covers the case where
/// an assertion panics first. A `Drop` implementation cannot await, so it owns a
/// single-threaded runtime on a thread of its own. The drop is idempotent, so
/// running it twice is harmless.
struct ScratchDatabase {
    name: String,
    admin: PgConnectOptions,
}

impl ScratchDatabase {
    /// The connect options for the scratch database itself.
    fn options(&self) -> PgConnectOptions {
        self.admin.clone().database(&self.name)
    }

    /// Drops this database, forcing out any session still attached to it.
    async fn remove(&self) -> Result<(), sqlx::Error> {
        drop_scratch_database(&self.admin, &self.name).await
    }
}

impl Drop for ScratchDatabase {
    fn drop(&mut self) {
        let admin = self.admin.clone();
        let name = self.name.clone();

        let cleanup = std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };

            runtime.block_on(async move {
                let _ = drop_scratch_database(&admin, &name).await;
            });
        });

        let _ = cleanup.join();
    }
}

/// Drops a database through a connection to the administration database.
async fn drop_scratch_database(admin: &PgConnectOptions, name: &str) -> Result<(), sqlx::Error> {
    let mut connection = PgConnection::connect_with(admin).await?;

    // `WITH (FORCE)` disconnects anything still attached: PostgreSQL 13 added
    // it, and this project's documented floor is 14.
    sqlx::query(&format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"))
        .execute(&mut connection)
        .await?;

    Ok(())
}

/// The connect options for the administration database of the suite.
fn admin_options() -> PgConnectOptions {
    PgConnectOptions::from_str(&common::test_database_url())
        .expect("the test database url must parse")
}

/// Creates an empty database with a name unique to this run.
async fn create_scratch_database() -> ScratchDatabase {
    let admin = admin_options();
    // The same rule the harness applies to the suite applies to every database
    // this suite creates: a name that contains `test` and nothing else.
    let name = format!("atom_test_scratch_{}", Uuid::new_v4().simple());
    assert!(
        name.contains("test"),
        "a database this suite creates must be recognisable as disposable"
    );

    let mut connection = PgConnection::connect_with(&admin)
        .await
        .expect("the administration database must be reachable");

    sqlx::query(&format!("CREATE DATABASE \"{name}\""))
        .execute(&mut connection)
        .await
        .expect("the atom role must be allowed to create a scratch database");

    ScratchDatabase { name, admin }
}

/// Whether a database with this name exists.
async fn database_exists(name: &str) -> Result<bool, String> {
    let mut connection = PgConnection::connect_with(&admin_options())
        .await
        .map_err(|error| format!("the administration database must be reachable: {error}"))?;

    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)")
        .bind(name)
        .fetch_one(&mut connection)
        .await
        .map_err(|error| format!("the catalogue must be readable: {error}"))
}

/// What an empty database looks like after the migrations ran.
#[derive(Debug)]
struct ScratchProbe {
    /// Tables in `public` before the migrations: the point is that it is zero.
    tables_before: i64,
    /// The versions recorded in `_sqlx_migrations`, oldest first.
    versions: Vec<i64>,
    company_table: bool,
    application_table: bool,
    event_table: bool,
    stage_type: bool,
    event_kind_type: bool,
}

/// Migrates a scratch database from empty and reports what is there.
async fn probe_scratch_database(scratch: &ScratchDatabase) -> Result<ScratchProbe, String> {
    let mut connection = PgConnection::connect_with(&scratch.options())
        .await
        .map_err(|error| format!("the scratch database must be reachable: {error}"))?;

    let tables_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public'",
    )
    .fetch_one(&mut connection)
    .await
    .map_err(|error| format!("the scratch database must be probeable: {error}"))?;

    sqlx::migrate!("./migrations")
        .run(&mut connection)
        .await
        .map_err(|error| format!("the migrations must apply to an empty database: {error}"))?;

    let versions: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&mut connection)
            .await
            .map_err(|error| format!("the applied migrations must be readable: {error}"))?;

    let (company_table, application_table, event_table): (bool, bool, bool) = sqlx::query_as(
        "SELECT to_regclass('companies') IS NOT NULL,
                to_regclass('job_applications') IS NOT NULL,
                to_regclass('application_events') IS NOT NULL",
    )
    .fetch_one(&mut connection)
    .await
    .map_err(|error| format!("the new tables must be probeable: {error}"))?;

    let (stage_type, event_kind_type): (bool, bool) = sqlx::query_as(
        "SELECT to_regtype('job_application_status') IS NOT NULL,
                to_regtype('application_event_kind') IS NOT NULL",
    )
    .fetch_one(&mut connection)
    .await
    .map_err(|error| format!("the new enum types must be probeable: {error}"))?;

    Ok(ScratchProbe {
        tables_before,
        versions,
        company_table,
        application_table,
        event_table,
        stage_type,
        event_kind_type,
    })
}

// ---------------------------------------------------------------------------
// The migration applies from nothing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_job_tables_apply_from_an_empty_database() {
    let scratch = create_scratch_database().await;

    // The fallible work is collected first and the database is dropped before
    // any assertion can panic, so a failure of the migration itself still
    // leaves no scratch database behind. `Drop` covers the rest.
    let probe = probe_scratch_database(&scratch).await;
    let removal = scratch.remove().await;
    let remains = database_exists(&scratch.name).await;

    let probe = probe.unwrap_or_else(|error| panic!("{error}"));
    if let Err(error) = removal {
        panic!(
            "the scratch database {:?} must be removable: {error}",
            scratch.name
        );
    }
    match remains {
        Ok(false) => {}
        Ok(true) => panic!(
            "the scratch database {:?} still exists after the drop",
            scratch.name
        ),
        Err(error) => panic!("{error}"),
    }

    assert_eq!(
        probe.tables_before, 0,
        "the scratch database must start empty; the migration was not proven against a fresh one"
    );
    assert_eq!(
        probe.versions,
        vec![1, 2, 3, 4, 5, 6, 7, 8],
        "an empty database must end up carrying every migration, including 0005 to 0007"
    );
    assert!(
        probe.company_table,
        "`companies` must exist after the migrations ran"
    );
    assert!(
        probe.application_table,
        "`job_applications` must exist after the migrations ran"
    );
    assert!(
        probe.event_table,
        "`application_events` must exist after the migrations ran"
    );
    assert!(
        probe.stage_type,
        "the `job_application_status` type must exist after the migrations ran"
    );
    assert!(
        probe.event_kind_type,
        "the `application_event_kind` type must exist after the migrations ran"
    );
}

// ---------------------------------------------------------------------------
// The company name: unique per owner, case-insensitively, and trimmed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_company_name_is_unique_per_owner_case_insensitively() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "jobs-company-unique-owner").await;
    let other = insert_account(&pool, "jobs-company-unique-other").await;

    let first = insert_company(&pool, owner, "Acme")
        .await
        .expect("a plain company name must be accepted");

    // The stored value keeps its casing: the index compares the folded name,
    // it does not rewrite the column.
    let stored: String = sqlx::query_scalar("SELECT name FROM companies WHERE id = $1")
        .bind(first)
        .fetch_one(&pool)
        .await
        .expect("the accepted company must be readable");
    assert_eq!(stored, "Acme", "the company name must round-trip unchanged");

    // Two spellings of the same name, both of which the folded index sees as
    // `acme`. Either would be a second company called Acme for this owner.
    for duplicate in ["acme", "ACME"] {
        let result = insert_company(&pool, owner, duplicate).await.map(|_| ());
        assert_rejected(result, "23505", "companies_user_id_lower_name_unique_idx");
    }

    // Another owner may hold the same name: the predicate is the owner, not
    // the name alone.
    let second_owner = insert_company(&pool, other, "Acme")
        .await
        .expect("two owners may each hold a company called Acme");
    assert!(
        !second_owner.is_nil(),
        "the second owner's company must have been inserted"
    );

    // Nothing was stored by the rejected inserts.
    let owned: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM companies WHERE user_id = $1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .expect("the companies of the fixture owner must be countable");
    assert_eq!(owned, 1, "a rejected insert must leave no row behind");
}

#[tokio::test]
async fn a_company_name_must_be_trimmed() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "jobs-company-trim").await;

    // The accepted control first, so the rejection below cannot pass against a
    // table that refuses every name.
    insert_company(&pool, owner, "Acme")
        .await
        .expect("a trimmed company name must be accepted");

    // Each name is unique once trimmed, so the trim check is the only
    // constraint that can reject it: a collision on the folded index would be
    // a different failure and would prove a different rule.
    for untrimmed in ["Globex ", " Initech", "Umbrella  "] {
        let result = insert_company(&pool, owner, untrimmed).await.map(|_| ());
        assert_rejected(result, "23514", "companies_name_trimmed_chk");
    }

    let owned: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM companies WHERE user_id = $1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .expect("the companies of the fixture owner must be countable");
    assert_eq!(owned, 1, "a rejected insert must leave no row behind");
}

// ---------------------------------------------------------------------------
// The composite key: an application cannot name another owner's company
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_application_cannot_belong_to_another_owners_company() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "jobs-application-owner").await;
    let other = insert_account(&pool, "jobs-application-other").await;

    let company = insert_company(&pool, owner, "Acme")
        .await
        .expect("the owner's company must be inserted");

    // The application is honest about its own owner and lies about the
    // company's: the direct `users` key is satisfied, so only the composite key
    // can reject it. That is exactly the row the composite key exists to make
    // unrepresentable.
    let cross_owner = insert_application(&pool, &NewApplication::new(other, company))
        .await
        .map(|_| ());
    assert_rejected(cross_owner, "23503", "job_applications_company_fk");

    // The same row with the company's owner is accepted, so the rejection above
    // is not caused by something else in the row.
    let accepted = insert_application(&pool, &NewApplication::new(owner, company))
        .await
        .expect("an application owned by the company's owner must be accepted");

    let applications: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM job_applications WHERE id = $1")
            .bind(accepted)
            .fetch_one(&pool)
            .await
            .expect("the fixture application must be countable");
    assert_eq!(
        applications, 1,
        "the cross-owner application must not be stored"
    );

    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM job_applications WHERE company_id = $1")
            .bind(company)
            .fetch_one(&pool)
            .await
            .expect("the applications of the fixture company must be countable");
    assert_eq!(total, 1, "only the accepted application may exist");
}

// ---------------------------------------------------------------------------
// Deletion behaviour
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_company_with_applications_cannot_be_deleted() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "jobs-company-delete").await;

    let company = insert_company(&pool, owner, "Acme")
        .await
        .expect("the company must be inserted");
    let application = insert_application(&pool, &NewApplication::new(owner, company))
        .await
        .expect("the application must be inserted");
    insert_event(&pool, owner, application, "note", 1)
        .await
        .expect("the event must be inserted");

    // `JOB-R06` is a `409`, and `NO ACTION` is what the database refuses with.
    let direct = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company)
        .execute(&pool)
        .await
        .map(|_| ());
    assert_rejected(direct, "23503", "job_applications_company_fk");

    // The refusal is about the applications, not about deletion in general: a
    // company with none is deletable.
    let empty = insert_company(&pool, owner, "Globex")
        .await
        .expect("the second company must be inserted");
    sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(empty)
        .execute(&pool)
        .await
        .expect("a company with no applications must be deletable");

    // The second half is the reason the key is `NO ACTION` and not `RESTRICT`.
    // Both refuse the direct delete above; only `NO ACTION` defers the check to
    // the end of the statement, so the account's cascade can remove the
    // applications and the company together and leave nothing to object.
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(owner)
        .execute(&pool)
        .await
        .expect("deleting an account that owns a company and its application must succeed");

    let companies: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM companies WHERE user_id = $1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .expect("the deleted owner's companies must be countable");
    let applications: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM job_applications WHERE user_id = $1")
            .bind(owner)
            .fetch_one(&pool)
            .await
            .expect("the deleted owner's applications must be countable");

    assert_eq!(companies, 0, "the account's companies must go with it");
    assert_eq!(
        applications, 0,
        "the account's applications must go with it"
    );
}

#[tokio::test]
async fn deleting_an_application_deletes_its_events() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "jobs-event-cascade").await;
    let other = insert_account(&pool, "jobs-event-bystander").await;

    let company = insert_company(&pool, owner, "Acme")
        .await
        .expect("the company must be inserted");
    let application = insert_application(&pool, &NewApplication::new(owner, company))
        .await
        .expect("the application must be inserted");
    for (index, kind) in ["note", "email", "call"].into_iter().enumerate() {
        insert_event(&pool, owner, application, kind, index as i64)
            .await
            .expect("the event must be inserted");
    }

    // Another account's timeline must survive: the cascade follows the
    // application, not the table.
    let bystander_company = insert_company(&pool, other, "Globex")
        .await
        .expect("the bystander's company must be inserted");
    let kept = insert_application(&pool, &NewApplication::new(other, bystander_company))
        .await
        .expect("the bystander's application must be inserted");
    insert_event(&pool, other, kept, "offer", 2)
        .await
        .expect("the bystander's event must be inserted");

    let before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM application_events WHERE application_id = $1")
            .bind(application)
            .fetch_one(&pool)
            .await
            .expect("the events must be countable");
    assert_eq!(before, 3, "the fixture must hold three events");

    sqlx::query("DELETE FROM job_applications WHERE id = $1 AND user_id = $2")
        .bind(application)
        .bind(owner)
        .execute(&pool)
        .await
        .expect("the application must be deletable");

    let after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM application_events WHERE application_id = $1")
            .bind(application)
            .fetch_one(&pool)
            .await
            .expect("the events must be countable after the delete");
    assert_eq!(
        after, 0,
        "deleting an application must delete its events through the composite key's cascade"
    );

    let kept_events: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM application_events WHERE application_id = $1")
            .bind(kept)
            .fetch_one(&pool)
            .await
            .expect("the bystander's events must be countable");
    assert_eq!(kept_events, 1, "another application's events must survive");

    // The bystander is not needed after the assertions; removing it keeps the
    // shared database tidy without touching anything this test did not create.
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(other)
        .execute(&pool)
        .await
        .expect("the bystander account must be removable");
}

#[tokio::test]
async fn deleting_an_account_deletes_its_companies_applications_and_events() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "jobs-account-delete").await;
    let bystander = insert_account(&pool, "jobs-account-bystander").await;

    let company = insert_company(&pool, owner, "Acme")
        .await
        .expect("the company must be inserted");
    let application = insert_application(&pool, &NewApplication::new(owner, company))
        .await
        .expect("the application must be inserted");
    insert_event(&pool, owner, application, "interview", 3)
        .await
        .expect("the event must be inserted");

    let kept_company = insert_company(&pool, bystander, "Globex")
        .await
        .expect("the bystander's company must be inserted");
    let kept_application = insert_application(&pool, &NewApplication::new(bystander, kept_company))
        .await
        .expect("the bystander's application must be inserted");
    insert_event(&pool, bystander, kept_application, "rejection", 4)
        .await
        .expect("the bystander's event must be inserted");

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(owner)
        .execute(&pool)
        .await
        .expect("the account must be deletable");

    for (table, column) in [
        ("companies", "user_id"),
        ("job_applications", "user_id"),
        ("application_events", "user_id"),
    ] {
        let remaining: i64 =
            sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE {column} = $1"))
                .bind(owner)
                .fetch_one(&pool)
                .await
                .expect("the deleted owner's rows must be countable");

        assert_eq!(
            remaining, 0,
            "deleting an account must delete its `{table}` rows"
        );
    }

    for (table, id) in [
        ("companies", kept_company),
        ("job_applications", kept_application),
    ] {
        let present: i64 =
            sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE id = $1"))
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("the bystander's row must be countable");

        assert_eq!(present, 1, "another account's `{table}` row must survive");
    }

    let kept_events: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM application_events WHERE application_id = $1")
            .bind(kept_application)
            .fetch_one(&pool)
            .await
            .expect("the bystander's events must be countable");
    assert_eq!(kept_events, 1, "another account's events must survive");

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(bystander)
        .execute(&pool)
        .await
        .expect("the bystander account must be removable");
}

// ---------------------------------------------------------------------------
// Salary
// ---------------------------------------------------------------------------

/// One salary shape the currency-required constraint must refuse.
///
/// A named struct rather than a tuple: four positional fields, three of them
/// the same `Option<&str>`, is exactly where a reader swaps two arguments
/// without noticing.
struct SalaryShape {
    /// What the shape is, for the failure message.
    description: &'static str,
    salary_min: Option<&'static str>,
    salary_max: Option<&'static str>,
    salary_currency: Option<&'static str>,
}

#[tokio::test]
async fn a_salary_bound_requires_a_currency() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "jobs-salary-currency").await;
    let company = insert_company(&pool, owner, "Acme")
        .await
        .expect("the company must be inserted");

    // Both accepted shapes first: no bounds and no currency, and a complete
    // range with its currency.
    insert_application(&pool, &NewApplication::new(owner, company))
        .await
        .expect("an application with no salary must be accepted");
    insert_application(
        &pool,
        &NewApplication {
            salary_min: Some("1000"),
            salary_max: Some("2000"),
            salary_currency: Some("USD"),
            ..NewApplication::new(owner, company)
        },
    )
    .await
    .expect("a complete salary range with its currency must be accepted");

    // A bound without a currency, in each of the three directions the
    // constraint covers: a minimum only, a maximum only, and a currency with
    // no bound at all.
    let shapes: [SalaryShape; 3] = [
        SalaryShape {
            description: "a minimum only",
            salary_min: Some("1000"),
            salary_max: None,
            salary_currency: None,
        },
        SalaryShape {
            description: "a maximum only",
            salary_min: None,
            salary_max: Some("2000"),
            salary_currency: None,
        },
        SalaryShape {
            description: "a currency with no bound",
            salary_min: None,
            salary_max: None,
            salary_currency: Some("USD"),
        },
    ];

    for shape in shapes {
        let result = insert_application(
            &pool,
            &NewApplication {
                salary_min: shape.salary_min,
                salary_max: shape.salary_max,
                salary_currency: shape.salary_currency,
                ..NewApplication::new(owner, company)
            },
        )
        .await
        .map(|_| ());

        assert_eq!(
            rejected(result, "23514"),
            Some("job_applications_salary_currency_required_chk".to_string()),
            "{} must be refused by the currency-required constraint",
            shape.description
        );
    }

    let stored: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM job_applications WHERE company_id = $1")
            .bind(company)
            .fetch_one(&pool)
            .await
            .expect("the fixture applications must be countable");
    assert_eq!(stored, 2, "a rejected insert must leave no row behind");
}

#[tokio::test]
async fn a_salary_minimum_above_the_maximum_is_refused() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "jobs-salary-range").await;
    let company = insert_company(&pool, owner, "Acme")
        .await
        .expect("the company must be inserted");

    // A single-point range is legal: the constraint is `<=`, not `<`.
    insert_application(
        &pool,
        &NewApplication {
            salary_min: Some("1000.0000"),
            salary_max: Some("1000.0000"),
            salary_currency: Some("USD"),
            ..NewApplication::new(owner, company)
        },
    )
    .await
    .expect("a range whose bounds are equal must be accepted");

    let inverted = insert_application(
        &pool,
        &NewApplication {
            salary_min: Some("2000"),
            salary_max: Some("1000"),
            salary_currency: Some("USD"),
            ..NewApplication::new(owner, company)
        },
    )
    .await
    .map(|_| ());
    assert_rejected(inverted, "23514", "job_applications_salary_range_chk");

    // The sign is a separate constraint with its own name; a negative bound is
    // refused even when the range itself is ordered.
    let negative = insert_application(
        &pool,
        &NewApplication {
            salary_min: Some("-1"),
            salary_max: Some("1000"),
            salary_currency: Some("USD"),
            ..NewApplication::new(owner, company)
        },
    )
    .await
    .map(|_| ());
    assert_rejected(
        negative,
        "23514",
        "job_applications_salary_non_negative_chk",
    );

    let stored: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM job_applications WHERE company_id = $1")
            .bind(company)
            .fetch_one(&pool)
            .await
            .expect("the fixture applications must be countable");
    assert_eq!(stored, 1, "a rejected insert must leave no row behind");
}

#[tokio::test]
async fn money_is_stored_with_four_decimal_places() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "jobs-money").await;
    let company = insert_company(&pool, owner, "Acme")
        .await
        .expect("the company must be inserted");

    let application = insert_application(
        &pool,
        &NewApplication {
            salary_min: Some("1490.5"),
            salary_max: Some("2000"),
            salary_currency: Some("USD"),
            ..NewApplication::new(owner, company)
        },
    )
    .await
    .expect("a decimal salary must be inserted");

    // `::text` is the wire form `C-J1` pins: the API carries money as text, so
    // the stored scale and the published scale are the same four places.
    let (stored_min, stored_max): (String, String) = sqlx::query_as(
        "SELECT salary_min::text, salary_max::text FROM job_applications WHERE id = $1",
    )
    .bind(application)
    .fetch_one(&pool)
    .await
    .expect("the application must be readable");

    assert_eq!(
        stored_min, "1490.5000",
        "a salary minimum is stored at scale 4"
    );
    assert_eq!(
        stored_max, "2000.0000",
        "a salary maximum is stored at scale 4"
    );

    // More than four decimals: PostgreSQL does not reject it, it rounds it to
    // the column's scale. The assertion pins the behaviour actually observed on
    // this server rather than assuming a rejection.
    let rounded = insert_application(
        &pool,
        &NewApplication {
            salary_min: Some("149.876549"),
            salary_currency: Some("USD"),
            ..NewApplication::new(owner, company)
        },
    )
    .await
    .expect("a value with six decimals must be accepted and rounded");

    let rounded_min: String =
        sqlx::query_scalar("SELECT salary_min::text FROM job_applications WHERE id = $1")
            .bind(rounded)
            .fetch_one(&pool)
            .await
            .expect("the rounded application must be readable");
    assert_eq!(
        rounded_min, "149.8765",
        "a value with more than four decimals must be rounded to four places"
    );
}

// ---------------------------------------------------------------------------
// The stage vocabulary
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_unknown_stage_is_unrepresentable() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "jobs-stage").await;
    let company = insert_company(&pool, owner, "Acme")
        .await
        .expect("the company must be inserted");

    // Every stage the contract names must be representable, so the rejection
    // below cannot pass against a type that refuses everything.
    for stage in STAGES {
        let stored: String = sqlx::query_scalar("SELECT $1::text::job_application_status::text")
            .bind(stage)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|error| panic!("the stage {stage:?} must be representable: {error}"));
        assert_eq!(stored, stage, "the stage must round-trip unchanged");

        insert_application(
            &pool,
            &NewApplication {
                status: stage,
                ..NewApplication::new(owner, company)
            },
        )
        .await
        .unwrap_or_else(|error| {
            panic!("an application in the stage {stage:?} must be storable: {error}")
        });
    }

    // An unknown label has no enum value to cast to. `22P02` is
    // invalid_text_representation: the type refuses the token, so there is no
    // constraint to name. That is the point of the enum over a `CHECK` on text.
    let cast = sqlx::query_scalar::<_, String>("SELECT $1::text::job_application_status::text")
        .bind("onsite")
        .fetch_one(&pool)
        .await
        .map(|_| ());
    assert_eq!(
        rejected(cast, "22P02"),
        None,
        "an unknown stage must be refused by the enum type, which names no constraint"
    );

    // The same through the column, which is the path a careless writer takes.
    let inserted = insert_application(
        &pool,
        &NewApplication {
            status: "onsite",
            ..NewApplication::new(owner, company)
        },
    )
    .await
    .map(|_| ());
    assert_eq!(
        rejected(inserted, "22P02"),
        None,
        "an insert of an unknown stage must be refused by the enum type"
    );

    let stored: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM job_applications WHERE company_id = $1")
            .bind(company)
            .fetch_one(&pool)
            .await
            .expect("the fixture applications must be countable");
    assert_eq!(
        stored,
        STAGES.len() as i64,
        "only the seven accepted stages may exist, and no rejected row may be stored"
    );
}
