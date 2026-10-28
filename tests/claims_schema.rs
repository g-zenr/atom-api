//! Integration tests for the reimbursement schema (`migrations/0004`).
//!
//! The chain `C-R1` adds two tables and no endpoint, so there is no router to
//! drive: these tests speak SQL to the real, disposable database. That is the
//! only way to prove a constraint, because a `CHECK`, a composite foreign key
//! and an `ON DELETE` action are properties of PostgreSQL, not of this crate.
//!
//! Every rejection asserts the SQLSTATE and, where the constraint is the
//! subject, the constraint name the server reports. "An error happened" is not
//! evidence that the constraint under test is the thing that fired.
//!
//! The tests are independent. Each fixture is built from a fresh UUID and a
//! unique email address, and each assertion is restricted to the ids the test
//! created, so no test depends on the order another one ran in and none
//! truncates a shared table. The one shared resource is the disposable test
//! database itself, which the harness refuses to use unless its name contains
//! `test`.
//!
//! The contract is `C-R1` in `docs/engineering/runs/archive/phase-2/ledger.md`; the
//! constraints are defined in `docs/database/schema.md` §11.1 and §11.2.

pub mod common;

use std::str::FromStr;

use chrono::{DateTime, TimeZone, Utc};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};
use uuid::Uuid;

/// The currency every fixture starts from.
const USD: &str = "USD";

/// The draft claim's title. Length is a separate constraint with its own test
/// in `R2`; here it only has to be valid.
const CLAIM_TITLE: &str = "a reimbursement claim";

/// Two timestamps, ordered, for the lifecycle tests.
fn base_times() -> (DateTime<Utc>, DateTime<Utc>, DateTime<Utc>) {
    let submitted = Utc
        .with_ymd_and_hms(2026, 3, 1, 9, 0, 0)
        .single()
        .expect("the fixture timestamp must be valid");
    let decided = Utc
        .with_ymd_and_hms(2026, 3, 2, 9, 0, 0)
        .single()
        .expect("the fixture timestamp must be valid");
    let paid = Utc
        .with_ymd_and_hms(2026, 3, 3, 9, 0, 0)
        .single()
        .expect("the fixture timestamp must be valid");

    (submitted, decided, paid)
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
    .bind("claims-schema-fixture-not-a-real-hash")
    .bind("Claims Schema Fixture")
    .execute(pool)
    .await
    .expect("the fixture account must be inserted");

    id
}

/// Inserts a project owned by `user_id` and returns its id.
async fn insert_project(pool: &PgPool, user_id: Uuid, name: &str) -> Uuid {
    let id = Uuid::new_v4();

    sqlx::query("INSERT INTO projects (id, user_id, name) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(user_id)
        .bind(name)
        .execute(pool)
        .await
        .expect("the fixture project must be inserted");

    id
}

/// One `expense_claims` row as a test writes it.
///
/// `status` is the wire token and the insert casts it to `expense_claim_status`,
/// so a misspelt status fails at the server rather than being stored as text.
/// `total_amount` is carried as a decimal string because SQLx cannot decode
/// `NUMERIC` without a decimal crate (ADR-0007), and the same text form is what
/// the API will publish.
struct NewClaim {
    user_id: Uuid,
    currency: &'static str,
    total_amount: &'static str,
    status: &'static str,
    project_id: Option<Uuid>,
    submitted_at: Option<DateTime<Utc>>,
    decided_at: Option<DateTime<Utc>>,
    paid_at: Option<DateTime<Utc>>,
}

impl NewClaim {
    /// A draft claim with no timestamps and a zero total: the only shape the
    /// lifecycle constraint accepts without further information.
    fn draft(user_id: Uuid) -> Self {
        Self {
            user_id,
            currency: USD,
            total_amount: "0",
            status: "draft",
            project_id: None,
            submitted_at: None,
            decided_at: None,
            paid_at: None,
        }
    }
}

/// Inserts a claim and returns its id, or the error the server returned.
///
/// The money parameter is cast through `text` so that SQLx sends a string;
/// a direct `$5::numeric` would make SQLx demand a decimal type that the crate
/// deliberately does not depend on.
async fn insert_claim(pool: &PgPool, claim: &NewClaim) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO expense_claims
            (id, user_id, title, currency, total_amount, status,
             project_id, submitted_at, decided_at, paid_at)
         VALUES ($1, $2, $3, $4, $5::text::numeric, $6::text::expense_claim_status,
                 $7, $8, $9, $10)",
    )
    .bind(id)
    .bind(claim.user_id)
    .bind(CLAIM_TITLE)
    .bind(claim.currency)
    .bind(claim.total_amount)
    .bind(claim.status)
    .bind(claim.project_id)
    .bind(claim.submitted_at)
    .bind(claim.decided_at)
    .bind(claim.paid_at)
    .execute(pool)
    .await?;

    Ok(id)
}

/// Inserts a line item and returns its id, or the error the server returned.
///
/// `user_id` is a parameter rather than being read from the claim: the whole
/// point of the composite key is that a caller can pass a value that disagrees
/// with the claim's owner, and the database has to reject it.
async fn insert_item(
    pool: &PgPool,
    user_id: Uuid,
    claim_id: Uuid,
    amount: &'static str,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO expense_items
            (id, user_id, claim_id, description, amount, incurred_on)
         VALUES ($1, $2, $3, $4, $5::text::numeric, DATE '2026-03-01')",
    )
    .bind(id)
    .bind(user_id)
    .bind(claim_id)
    .bind("a fixture line item")
    .bind(amount)
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
    claim_table: bool,
    item_table: bool,
    status_type: bool,
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

    let (claim_table, item_table): (bool, bool) = sqlx::query_as(
        "SELECT to_regclass('expense_claims') IS NOT NULL,
                to_regclass('expense_items') IS NOT NULL",
    )
    .fetch_one(&mut connection)
    .await
    .map_err(|error| format!("the new tables must be probeable: {error}"))?;

    let status_type: bool =
        sqlx::query_scalar("SELECT to_regtype('expense_claim_status') IS NOT NULL")
            .fetch_one(&mut connection)
            .await
            .map_err(|error| format!("the new enum must be probeable: {error}"))?;

    Ok(ScratchProbe {
        tables_before,
        versions,
        claim_table,
        item_table,
        status_type,
    })
}

// ---------------------------------------------------------------------------
// The migration applies from nothing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_expense_claim_tables_apply_from_an_empty_database() {
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
        vec![1, 2, 3, 4, 5, 6, 7],
        "an empty database must end up carrying every migration in the chain, \
         including 0004 to 0007"
    );
    assert!(
        probe.claim_table,
        "`expense_claims` must exist after the migrations ran"
    );
    assert!(
        probe.item_table,
        "`expense_items` must exist after the migrations ran"
    );
    assert!(
        probe.status_type,
        "the `expense_claim_status` type must exist after the migrations ran"
    );
}

// ---------------------------------------------------------------------------
// Currency
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_claim_rejects_a_currency_that_is_not_three_upper_case_letters() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "claims-currency").await;

    // The accepted value first, so the test cannot pass against a table that
    // rejects every currency.
    let accepted = insert_claim(&pool, &NewClaim::draft(owner))
        .await
        .expect("a three-letter upper-case currency must be accepted");
    let stored: String = sqlx::query_scalar("SELECT currency FROM expense_claims WHERE id = $1")
        .bind(accepted)
        .fetch_one(&pool)
        .await
        .expect("the accepted claim must be readable");
    assert_eq!(stored, USD, "the currency must round-trip unchanged");

    // `USDX` is a different failure from the two below, and it is not the
    // constraint `C-R1` names. `currency` is `CHAR(3)`, so PostgreSQL rejects a
    // four-character value with `22001` (string_data_right_truncation) while
    // coercing it into the column, before any `CHECK` is evaluated. Measured on
    // PostgreSQL 18.1. The value is still unrepresentable, which is what the
    // contract is for; only the mechanism differs, so the test pins the
    // mechanism that actually runs.
    let four_characters = insert_claim(
        &pool,
        &NewClaim {
            currency: "USDX",
            ..NewClaim::draft(owner)
        },
    )
    .await
    .map(|_| ());
    assert_eq!(
        rejected(four_characters, "22001"),
        None,
        "a four-character currency is rejected by the CHAR(3) type, not by a named constraint"
    );

    for currency in ["usd", "US"] {
        let result = insert_claim(
            &pool,
            &NewClaim {
                currency,
                ..NewClaim::draft(owner)
            },
        )
        .await
        .map(|_| ());

        assert_rejected(result, "23514", "expense_claims_currency_chk");
    }

    // Nothing was stored by the rejected inserts.
    let claims: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM expense_claims WHERE user_id = $1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .expect("the claims of the fixture owner must be countable");
    assert_eq!(claims, 1, "a rejected insert must leave no row behind");
}

// ---------------------------------------------------------------------------
// Money
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_claim_rejects_a_negative_total() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "claims-negative-total").await;

    // Zero is the documented default and must be accepted, so the rejection
    // below is not "the column refuses every number".
    sqlx::query("INSERT INTO expense_claims (id, user_id, title, currency, total_amount) VALUES ($1, $2, $3, $4, DEFAULT)")
        .bind(Uuid::new_v4())
        .bind(owner)
        .bind(CLAIM_TITLE)
        .bind(USD)
        .execute(&pool)
        .await
        .expect("the default total of zero must be accepted");

    let negative = insert_claim(
        &pool,
        &NewClaim {
            total_amount: "-0.01",
            ..NewClaim::draft(owner)
        },
    )
    .await
    .map(|_| ());

    assert_rejected(negative, "23514", "expense_claims_total_amount_chk");
}

// ---------------------------------------------------------------------------
// The lifecycle constraint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_lifecycle_constraint_rejects_a_submitted_claim_without_a_submission_timestamp() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "claims-lifecycle-reject").await;
    let (submitted, _, paid) = base_times();

    // A submitted claim carries a submission timestamp and nothing else, so the
    // timestamp-order constraint is satisfied vacuously and only the lifecycle
    // constraint can reject the row below.
    let accepted = insert_claim(
        &pool,
        &NewClaim {
            status: "submitted",
            submitted_at: Some(submitted),
            ..NewClaim::draft(owner)
        },
    )
    .await
    .expect("a submitted claim with its submission timestamp must be accepted");
    assert!(!accepted.is_nil());

    let without_submission = insert_claim(
        &pool,
        &NewClaim {
            status: "submitted",
            ..NewClaim::draft(owner)
        },
    )
    .await
    .map(|_| ());
    assert_rejected(
        without_submission,
        "23514",
        "expense_claims_lifecycle_timestamps_chk",
    );

    // The same for `paid`: the payment date is set, the decision is not.
    let paid_without_decision = insert_claim(
        &pool,
        &NewClaim {
            status: "paid",
            submitted_at: Some(submitted),
            decided_at: None,
            paid_at: Some(paid),
            ..NewClaim::draft(owner)
        },
    )
    .await
    .map(|_| ());

    // Two constraints can see this row: the lifecycle constraint (the `paid`
    // arm needs `decided_at`) and the timestamp-order constraint (`paid_at`
    // needs `decided_at`). Which one PostgreSQL evaluates first is not part of
    // any contract, so the SQLSTATE is pinned and the constraint must be one of
    // the two.
    let reported = rejected(paid_without_decision, "23514")
        .expect("a constraint violation must name its constraint");
    assert!(
        reported == "expense_claims_lifecycle_timestamps_chk"
            || reported == "expense_claims_timestamp_order_chk",
        "a paid claim without a decision must be rejected by one of the two lifecycle \
         constraints, not by {reported}"
    );

    // With `paid_at` null the order constraint is satisfied vacuously, so only
    // the lifecycle constraint can reject this row. This is the case that pins
    // `paid` to its required `decided_at`.
    let decision_alone = insert_claim(
        &pool,
        &NewClaim {
            status: "paid",
            submitted_at: Some(submitted),
            paid_at: None,
            ..NewClaim::draft(owner)
        },
    )
    .await
    .map(|_| ());
    assert_rejected(
        decision_alone,
        "23514",
        "expense_claims_lifecycle_timestamps_chk",
    );

    // The rejected rows left nothing behind.
    let claims: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM expense_claims WHERE user_id = $1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .expect("the claims of the fixture owner must be countable");
    assert_eq!(claims, 1, "only the accepted claim may exist");
}

#[tokio::test]
async fn the_lifecycle_constraint_accepts_every_consistent_status() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "claims-lifecycle-accept").await;
    let (submitted, decided, paid) = base_times();

    let shapes = [
        ("draft", None, None, None),
        ("submitted", Some(submitted), None, None),
        ("approved", Some(submitted), Some(decided), None),
        ("rejected", Some(submitted), Some(decided), None),
        ("paid", Some(submitted), Some(decided), Some(paid)),
    ];

    for (status, submitted_at, decided_at, paid_at) in shapes {
        let id = insert_claim(
            &pool,
            &NewClaim {
                status,
                total_amount: "1.0000",
                submitted_at,
                decided_at,
                paid_at,
                ..NewClaim::draft(owner)
            },
        )
        .await
        .unwrap_or_else(|error| {
            panic!("the `{status}` status must be storable with its own timestamps: {error}")
        });

        let stored: (String, bool, bool, bool) = sqlx::query_as(
            "SELECT status::text, submitted_at IS NOT NULL, decided_at IS NOT NULL,
                    paid_at IS NOT NULL
             FROM expense_claims WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("the accepted claim must be readable");

        assert_eq!(stored.0, status, "the status must round-trip unchanged");
        assert_eq!(
            (stored.1, stored.2, stored.3),
            (
                submitted_at.is_some(),
                decided_at.is_some(),
                paid_at.is_some()
            ),
            "the timestamps of a `{status}` claim must be stored as given"
        );
    }

    let claims: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM expense_claims WHERE user_id = $1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .expect("the claims of the fixture owner must be countable");
    assert_eq!(claims, 5, "one claim per status must be storable");
}

// ---------------------------------------------------------------------------
// The composite key: an item cannot claim one owner and belong to another
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_item_cannot_belong_to_another_owners_claim() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "claims-item-owner").await;
    let other = insert_account(&pool, "claims-item-other").await;

    let claim = insert_claim(&pool, &NewClaim::draft(owner))
        .await
        .expect("the owner's claim must be inserted");

    // The item is honest about its own owner and lies about the claim's: the
    // direct `users` key is satisfied, so only the composite key can reject it.
    // That is exactly the row the composite key exists to make unrepresentable.
    let cross_owner = insert_item(&pool, other, claim, "10.0000")
        .await
        .map(|_| ());
    assert_rejected(cross_owner, "23503", "expense_items_claim_fk");

    // The same row with the claim's owner is accepted, so the rejection above
    // is not caused by something else in the row.
    insert_item(&pool, owner, claim, "10.0000")
        .await
        .expect("an item owned by the claim's owner must be accepted");

    let items: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM expense_items WHERE claim_id = $1")
        .bind(claim)
        .fetch_one(&pool)
        .await
        .expect("the items of the fixture claim must be countable");
    assert_eq!(items, 1, "the cross-owner item must not have been stored");
}

// ---------------------------------------------------------------------------
// Deletion behaviour
// ---------------------------------------------------------------------------

#[tokio::test]
async fn deleting_a_claim_deletes_its_items() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "claims-cascade").await;

    let claim = insert_claim(&pool, &NewClaim::draft(owner))
        .await
        .expect("the claim must be inserted");
    for amount in ["1.0000", "2.5000", "3.2500"] {
        insert_item(&pool, owner, claim, amount)
            .await
            .expect("the item must be inserted");
    }

    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM expense_items WHERE claim_id = $1")
        .bind(claim)
        .fetch_one(&pool)
        .await
        .expect("the items must be countable");
    assert_eq!(before, 3, "the fixture must hold three items");

    sqlx::query("DELETE FROM expense_claims WHERE id = $1 AND user_id = $2")
        .bind(claim)
        .bind(owner)
        .execute(&pool)
        .await
        .expect("the claim must be deletable");

    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM expense_items WHERE claim_id = $1")
        .bind(claim)
        .fetch_one(&pool)
        .await
        .expect("the items must be countable after the delete");
    assert_eq!(
        after, 0,
        "deleting a claim must delete its items through the composite key's cascade"
    );
}

#[tokio::test]
async fn deleting_a_project_clears_the_claim_link_and_keeps_the_claim() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "claims-project-delete").await;
    let project = insert_project(&pool, owner, "the linked project").await;

    let claim = insert_claim(
        &pool,
        &NewClaim {
            project_id: Some(project),
            ..NewClaim::draft(owner)
        },
    )
    .await
    .expect("a claim linked to a project must be inserted");

    insert_item(&pool, owner, claim, "12.3400")
        .await
        .expect("the item must be inserted");

    sqlx::query("DELETE FROM projects WHERE id = $1 AND user_id = $2")
        .bind(project)
        .bind(owner)
        .execute(&pool)
        .await
        .expect("the project must be deletable");

    let stored: Option<(bool, Option<Uuid>)> =
        sqlx::query_as("SELECT TRUE, project_id FROM expense_claims WHERE id = $1")
            .bind(claim)
            .fetch_optional(&pool)
            .await
            .expect("the claim must be readable after the project delete");

    let (present, project_id) =
        stored.unwrap_or_else(|| panic!("deleting a project must not delete the claim"));
    assert!(present);
    assert_eq!(
        project_id, None,
        "the claim must survive the project delete with its link cleared"
    );

    let items: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM expense_items WHERE claim_id = $1")
        .bind(claim)
        .fetch_one(&pool)
        .await
        .expect("the items of the surviving claim must be countable");
    assert_eq!(items, 1, "the claim's items must survive with it");
}

#[tokio::test]
async fn deleting_an_account_deletes_its_claims_and_items() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "claims-account-delete").await;
    let bystander = insert_account(&pool, "claims-account-bystander").await;

    let claim = insert_claim(&pool, &NewClaim::draft(owner))
        .await
        .expect("the claim must be inserted");
    insert_item(&pool, owner, claim, "4.0000")
        .await
        .expect("the item must be inserted");

    // A second account's claim must survive: the cascade follows the owner, not
    // the table.
    let kept_claim = insert_claim(&pool, &NewClaim::draft(bystander))
        .await
        .expect("the bystander's claim must be inserted");
    insert_item(&pool, bystander, kept_claim, "5.0000")
        .await
        .expect("the bystander's item must be inserted");

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(owner)
        .execute(&pool)
        .await
        .expect("the account must be deletable");

    let claims: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM expense_claims WHERE user_id = $1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .expect("the deleted owner's claims must be countable");
    let items: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM expense_items WHERE user_id = $1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .expect("the deleted owner's items must be countable");

    assert_eq!(claims, 0, "deleting an account must delete its claims");
    assert_eq!(items, 0, "deleting an account must delete its items");

    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM expense_claims WHERE id = $1")
            .bind(kept_claim)
            .fetch_one(&pool)
            .await
            .expect("the bystander's claim must be countable"),
        1,
        "another account's claim must survive"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM expense_items WHERE claim_id = $1")
            .bind(kept_claim)
            .fetch_one(&pool)
            .await
            .expect("the bystander's items must be countable"),
        1,
        "another account's items must survive"
    );

    // The bystander is not needed after the assertions; removing it keeps the
    // shared database tidy without touching anything this test did not create.
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(bystander)
        .execute(&pool)
        .await
        .expect("the bystander account must be removable");
}

// ---------------------------------------------------------------------------
// Money at rest
// ---------------------------------------------------------------------------

#[tokio::test]
async fn money_is_stored_with_four_decimal_places() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "claims-money").await;

    let claim = insert_claim(
        &pool,
        &NewClaim {
            total_amount: "149.8",
            ..NewClaim::draft(owner)
        },
    )
    .await
    .expect("a decimal amount must be inserted");
    let item = insert_item(&pool, owner, claim, "149.8")
        .await
        .expect("a decimal amount must be inserted");

    // `::text` is the wire form `C-R1` pins: the API carries money as text, so
    // the stored scale and the published scale are the same four places.
    let stored_total: String =
        sqlx::query_scalar("SELECT total_amount::text FROM expense_claims WHERE id = $1")
            .bind(claim)
            .fetch_one(&pool)
            .await
            .expect("the claim must be readable");
    let stored_amount: String =
        sqlx::query_scalar("SELECT amount::text FROM expense_items WHERE id = $1")
            .bind(item)
            .fetch_one(&pool)
            .await
            .expect("the item must be readable");

    assert_eq!(
        stored_total, "149.8000",
        "a claim total is stored at scale 4"
    );
    assert_eq!(
        stored_amount, "149.8000",
        "an item amount is stored at scale 4"
    );

    // More than four decimals: PostgreSQL does not reject it, it rounds it to
    // the column's scale (measured on 18.1, and observed here rather than
    // assumed). Both statements below would fail if the column ever started
    // rejecting the value instead, which is the other behaviour the contract
    // allows.
    let rounded = insert_claim(
        &pool,
        &NewClaim {
            total_amount: "149.876549",
            ..NewClaim::draft(owner)
        },
    )
    .await
    .expect("a value with six decimals must be accepted and rounded");
    let rounded_total: String =
        sqlx::query_scalar("SELECT total_amount::text FROM expense_claims WHERE id = $1")
            .bind(rounded)
            .fetch_one(&pool)
            .await
            .expect("the rounded claim must be readable");
    assert_eq!(
        rounded_total, "149.8765",
        "a value with more than four decimals must be rounded to four places"
    );

    let half = insert_item(&pool, owner, claim, "0.00005")
        .await
        .expect("a value with five decimals must be accepted and rounded");
    let half_amount: String =
        sqlx::query_scalar("SELECT amount::text FROM expense_items WHERE id = $1")
            .bind(half)
            .fetch_one(&pool)
            .await
            .expect("the rounded item must be readable");
    assert_eq!(
        half_amount, "0.0001",
        "a half unit in the fifth place must round away from zero"
    );
}
