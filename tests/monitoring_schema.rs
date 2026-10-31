//! Integration tests for the price-monitoring schema (`migrations/0008`).
//!
//! The six tables are schema and rules before they are endpoints, so the
//! constraint-level evidence is spoken in SQL to the real, disposable
//! database — the same shape `tests/claims_schema.rs` uses. A `CHECK`, a
//! composite foreign key and an `ON DELETE` action are properties of
//! PostgreSQL, not of this crate.
//!
//! Every rejection asserts the SQLSTATE and, where the constraint is the
//! subject, the constraint name the server reports. "An error happened" is
//! not evidence that the constraint under test is the thing that fired.
//!
//! The tests are independent. Each fixture is built from fresh UUIDs and
//! unique email addresses, and each assertion is restricted to the ids the
//! test created, so no test depends on the order another one ran in and none
//! truncates a shared table. The one shared resource is the disposable test
//! database itself, which the harness refuses to use unless its name
//! contains `test`.
//!
//! The schema is defined in `docs/database/schema.md` §12 and in
//! `migrations/0008_price_monitoring.sql`, whose header comments explain
//! every choice these tests pin.

pub mod common;

use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};
use uuid::Uuid;

/// The currency every fixture starts from.
const EUR: &str = "EUR";

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
    .bind("monitoring-schema-fixture-not-a-real-hash")
    .bind("Monitoring Schema Fixture")
    .execute(pool)
    .await
    .expect("the fixture account must be inserted");

    id
}

/// Inserts a tracked product and returns its id.
///
/// The identity columns mirror what the service derives; the hash is a fixed
/// 64-character lower-case hex string, which is the only form the `CHECK`
/// accepts.
async fn insert_product(pool: &PgPool, user_id: Uuid, model: &str) -> Uuid {
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO tracked_products
            (id, user_id, brand, model, specs, normalized_name, config_hash)
         VALUES ($1, $2, 'Lenovo', $3, '', $4, '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef')",
    )
    .bind(id)
    .bind(user_id)
    .bind(model)
    .bind(format!("lenovo {}", model.to_lowercase()))
    .execute(pool)
    .await
    .expect("the fixture product must be inserted");

    id
}

/// One `product_listings` row as a test writes it.
struct NewListing<'a> {
    user_id: Uuid,
    product_id: Uuid,
    url: &'a str,
    currency: &'a str,
    availability: &'a str,
    validation_state: &'a str,
}

impl<'a> NewListing<'a> {
    /// A listing with every optional value at its default.
    fn new(user_id: Uuid, product_id: Uuid, url: &'a str) -> Self {
        Self {
            user_id,
            product_id,
            url,
            currency: EUR,
            availability: "unknown",
            validation_state: "valid",
        }
    }
}

/// Inserts a listing and returns its id, or the error the server returned.
async fn insert_listing(pool: &PgPool, listing: &NewListing<'_>) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO product_listings
            (id, user_id, product_id, retailer, url, currency, availability, validation_state)
         VALUES ($1, $2, $3, 'Fixture Retailer', $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(listing.user_id)
    .bind(listing.product_id)
    .bind(listing.url)
    .bind(listing.currency)
    .bind(listing.availability)
    .bind(listing.validation_state)
    .execute(pool)
    .await?;

    Ok(id)
}

/// One `price_observations` row as a test writes it.
///
/// `user_id` is a parameter rather than being read from the listing: the whole
/// point of the composite key is that a caller can pass a value that disagrees
/// with the listing's owner, and the database has to reject it.
struct NewObservation<'a> {
    user_id: Uuid,
    listing_id: Uuid,
    amount: &'a str,
    fingerprint: &'a str,
    run_id: Option<Uuid>,
}

/// Inserts an observation and returns its id, or the error the server
/// returned.
///
/// The money parameter is cast through `text` so that SQLx sends a string; a
/// direct `$5::numeric` would make SQLx demand a decimal type that the crate
/// deliberately does not depend on (`ADR-0007`).
async fn insert_observation(
    pool: &PgPool,
    observation: &NewObservation<'_>,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO price_observations
            (id, user_id, listing_id, amount, currency, observed_at, fingerprint, run_id)
         VALUES ($1, $2, $3, $4::text::numeric, 'EUR',
                 TIMESTAMPTZ '2026-10-30 09:00:00+00', $5, $6)",
    )
    .bind(id)
    .bind(observation.user_id)
    .bind(observation.listing_id)
    .bind(observation.amount)
    .bind(observation.fingerprint)
    .bind(observation.run_id)
    .execute(pool)
    .await?;

    Ok(id)
}

/// Inserts the product's one rule and returns its id.
async fn insert_rule(
    pool: &PgPool,
    user_id: Uuid,
    product_id: Uuid,
    target_amount: &str,
    percent_drop: Option<i32>,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO monitoring_rules
            (id, user_id, product_id, target_amount, target_currency, percent_drop)
         VALUES ($1, $2, $3, $4::text::numeric, 'EUR', $5)",
    )
    .bind(id)
    .bind(user_id)
    .bind(product_id)
    .bind(target_amount)
    .bind(percent_drop)
    .execute(pool)
    .await?;

    Ok(id)
}

/// Inserts an automation run and returns its id.
async fn insert_run(pool: &PgPool, user_id: Uuid) -> Uuid {
    let id = Uuid::new_v4();

    sqlx::query("INSERT INTO automation_runs (id, user_id) VALUES ($1, $2)")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("the fixture run must be inserted");

    id
}

/// Inserts an alert and returns its id, or the error the server returned.
async fn insert_alert(
    pool: &PgPool,
    user_id: Uuid,
    rule_id: Uuid,
    observation_id: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO price_alerts (id, user_id, rule_id, observation_id)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(user_id)
    .bind(rule_id)
    .bind(observation_id)
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
    tables: Vec<bool>,
}

/// The six tables migration `0008` must leave behind.
const MONITORING_TABLES: [&str; 6] = [
    "tracked_products",
    "product_listings",
    "price_observations",
    "monitoring_rules",
    "automation_runs",
    "price_alerts",
];

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

    let mut tables = Vec::new();
    for table in MONITORING_TABLES {
        let present: bool =
            sqlx::query_scalar(&format!("SELECT to_regclass('{table}') IS NOT NULL"))
                .fetch_one(&mut connection)
                .await
                .map_err(|error| format!("the new tables must be probeable: {error}"))?;
        tables.push(present);
    }

    Ok(ScratchProbe {
        tables_before,
        versions,
        tables,
    })
}

// ---------------------------------------------------------------------------
// The migration applies from nothing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_price_monitoring_tables_apply_from_an_empty_database() {
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
        "an empty database must end up carrying every migration in the chain, \
         including 0008"
    );
    for (present, table) in probe.tables.iter().zip(MONITORING_TABLES) {
        assert!(present, "`{table}` must exist after the migrations ran");
    }
}

// ---------------------------------------------------------------------------
// The idempotency index
// ---------------------------------------------------------------------------

/// The idempotency target of `ADR-0012` §Decision 5: a retried write of the
/// same fingerprint for the same listing of the same owner is refused by the
/// database itself, so no code path can double-write it.
#[tokio::test]
async fn a_repeated_fingerprint_for_one_listing_is_refused_by_the_database() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "mon-idempotency-owner").await;

    let product = insert_product(&pool, owner, "X1").await;
    let listing = insert_listing(
        &pool,
        &NewListing::new(owner, product, "https://fixture.test/idem"),
    )
    .await
    .expect("the fixture listing must be inserted");

    insert_observation(
        &pool,
        &NewObservation {
            user_id: owner,
            listing_id: listing,
            amount: "1299.0000",
            fingerprint: "retry-me",
            run_id: None,
        },
    )
    .await
    .expect("the first observation must be accepted");

    let retry = insert_observation(
        &pool,
        &NewObservation {
            user_id: owner,
            listing_id: listing,
            amount: "1299.0000",
            fingerprint: "retry-me",
            run_id: None,
        },
    )
    .await
    .map(|_| ());

    assert_rejected(
        retry,
        "23505",
        "price_observations_user_id_listing_fingerprint_unique_idx",
    );

    // The same fingerprint under a **different** listing of the same owner is
    // a different observation: the index is per listing.
    let other_listing = insert_listing(
        &pool,
        &NewListing::new(owner, product, "https://fixture.test/idem-2"),
    )
    .await
    .expect("the second fixture listing must be inserted");
    insert_observation(
        &pool,
        &NewObservation {
            user_id: owner,
            listing_id: other_listing,
            amount: "1299.0000",
            fingerprint: "retry-me",
            run_id: None,
        },
    )
    .await
    .expect("the same fingerprint under another listing is a different observation");

    let rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM price_observations WHERE user_id = $1")
            .bind(owner)
            .fetch_one(&pool)
            .await
            .expect("the fixture observations must be countable");
    assert_eq!(rows, 2, "one row per listing, not one row per fingerprint");
}

// ---------------------------------------------------------------------------
// The duplicate-alert index
// ---------------------------------------------------------------------------

/// Duplicate alerts are unrepresentable: one (rule, observation) pair is one
/// row, whatever writes it (`ADR-0012` §Decision 6).
#[tokio::test]
async fn one_rule_and_one_observation_cannot_raise_two_alerts() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "mon-alert-dupe-owner").await;

    let product = insert_product(&pool, owner, "X1").await;
    let listing = insert_listing(
        &pool,
        &NewListing::new(owner, product, "https://fixture.test/alert"),
    )
    .await
    .expect("the fixture listing must be inserted");
    let rule = insert_rule(&pool, owner, product, "500.0000", None)
        .await
        .expect("the fixture rule must be inserted");
    let observation = insert_observation(
        &pool,
        &NewObservation {
            user_id: owner,
            listing_id: listing,
            amount: "499.0000",
            fingerprint: "alert-dupe",
            run_id: None,
        },
    )
    .await
    .expect("the fixture observation must be inserted");

    insert_alert(&pool, owner, rule, observation)
        .await
        .expect("the first alert must be accepted");

    let duplicate = insert_alert(&pool, owner, rule, observation)
        .await
        .map(|_| ());
    assert_rejected(duplicate, "23505", "price_alerts_rule_observation_unique");

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM price_alerts WHERE user_id = $1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .expect("the fixture alerts must be countable");
    assert_eq!(rows, 1, "the duplicate was refused, not stored");
}

// ---------------------------------------------------------------------------
// The controlled vocabularies
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_controlled_vocabularies_are_check_constraints() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "mon-vocab").await;
    let product = insert_product(&pool, owner, "X1").await;
    let listing = insert_listing(
        &pool,
        &NewListing::new(owner, product, "https://fixture.test/vocab"),
    )
    .await
    .expect("the fixture listing must be inserted");
    let rule = insert_rule(&pool, owner, product, "500.0000", None)
        .await
        .expect("the fixture rule must be inserted");
    let observation = insert_observation(
        &pool,
        &NewObservation {
            user_id: owner,
            listing_id: listing,
            amount: "499.0000",
            fingerprint: "vocab-observation",
            run_id: None,
        },
    )
    .await
    .expect("the fixture observation must be inserted");

    // The status set.
    let paused = sqlx::query("UPDATE tracked_products SET status = 'paused' WHERE id = $1")
        .bind(product)
        .execute(&pool)
        .await
        .expect("paused is in the status set");
    assert_eq!(
        paused.rows_affected(),
        1,
        "the accepted value must be storable"
    );

    let unknown_status =
        sqlx::query("UPDATE tracked_products SET status = 'archived' WHERE id = $1")
            .bind(product)
            .execute(&pool)
            .await
            .map(|_| ());
    assert_rejected(unknown_status, "23514", "tracked_products_status_chk");

    // The configuration hash is a 64-character lower-case hex digest and
    // nothing else: an upper-case digest would silently fork the identity.
    let upper_hash =
        sqlx::query("UPDATE tracked_products SET config_hash = UPPER(config_hash) WHERE id = $1")
            .bind(product)
            .execute(&pool)
            .await
            .map(|_| ());
    assert_rejected(upper_hash, "23514", "tracked_products_config_hash_chk");

    // The listing's two reports.
    for (column, value, constraint) in [
        (
            "availability",
            "purchasable",
            "product_listings_availability_chk",
        ),
        (
            "validation_state",
            "doubted",
            "product_listings_validation_state_chk",
        ),
    ] {
        let refused = sqlx::query(&format!(
            "UPDATE product_listings SET {column} = $2 WHERE id = $1"
        ))
        .bind(listing)
        .bind(value)
        .execute(&pool)
        .await
        .map(|_| ());
        assert_rejected(refused, "23514", constraint);
    }

    // The run's lifecycle: `running` without a finish, finished with one.
    let run = insert_run(&pool, owner).await;
    let finished_twice = sqlx::query("UPDATE automation_runs SET outcome = 'ok' WHERE id = $1")
        .bind(run)
        .execute(&pool)
        .await
        .map(|_| ());
    assert_rejected(finished_twice, "23514", "automation_runs_lifecycle_chk");

    let finish =
        sqlx::query("UPDATE automation_runs SET outcome = 'ok', finished_at = NOW() WHERE id = $1")
            .bind(run)
            .execute(&pool)
            .await
            .expect("a terminal outcome with its stamp must be accepted");
    assert_eq!(finish.rows_affected(), 1);

    let unknown_outcome = sqlx::query(
        "UPDATE automation_runs SET outcome = 'succeeded', finished_at = NOW() WHERE id = $1",
    )
    .bind(run)
    .execute(&pool)
    .await
    .map(|_| ());
    assert_rejected(unknown_outcome, "23514", "automation_runs_outcome_chk");

    // The rule's percent drop: 0 would fire on every observation and 100 can
    // never be reached.
    for refused_value in [0, 100] {
        let refused = sqlx::query("UPDATE monitoring_rules SET percent_drop = $2 WHERE id = $1")
            .bind(rule)
            .bind(refused_value)
            .execute(&pool)
            .await
            .map(|_| ());
        assert_rejected(refused, "23514", "monitoring_rules_percent_drop_chk");
    }

    // The alert's delivery state.
    let alert = insert_alert(&pool, owner, rule, observation)
        .await
        .expect("the fixture alert must be inserted");

    let unknown_status =
        sqlx::query("UPDATE price_alerts SET notification_status = 'sent' WHERE id = $1")
            .bind(alert)
            .execute(&pool)
            .await
            .map(|_| ());
    assert_rejected(
        unknown_status,
        "23514",
        "price_alerts_notification_status_chk",
    );
}

// ---------------------------------------------------------------------------
// Money
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_negative_price_is_refused_and_money_is_stored_at_scale_four() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "mon-money").await;
    let product = insert_product(&pool, owner, "X1").await;
    let listing = insert_listing(
        &pool,
        &NewListing::new(owner, product, "https://fixture.test/money"),
    )
    .await
    .expect("the fixture listing must be inserted");

    // The accepted value first, so the rejections below cannot pass against a
    // table that refuses every amount.
    let accepted = insert_observation(
        &pool,
        &NewObservation {
            user_id: owner,
            listing_id: listing,
            amount: "1499.8",
            fingerprint: "money-1",
            run_id: None,
        },
    )
    .await
    .expect("a decimal amount must be accepted");

    let stored: String =
        sqlx::query_scalar("SELECT amount::text FROM price_observations WHERE id = $1")
            .bind(accepted)
            .fetch_one(&pool)
            .await
            .expect("the accepted observation must be readable");
    assert_eq!(
        stored, "1499.8000",
        "an observation amount is stored at scale 4 (`NUMERIC(19,4)::text` is the wire form)"
    );

    let negative = insert_observation(
        &pool,
        &NewObservation {
            user_id: owner,
            listing_id: listing,
            amount: "-0.0100",
            fingerprint: "money-2",
            run_id: None,
        },
    )
    .await
    .map(|_| ());
    assert_rejected(negative, "23514", "price_observations_amount_chk");

    // A target below zero is refused by the same shape of rule.
    let negative_target = insert_rule(&pool, owner, product, "-1.0000", None)
        .await
        .map(|_| ());
    assert_rejected(
        negative_target,
        "23514",
        "monitoring_rules_target_amount_chk",
    );

    // More than four decimals: PostgreSQL rounds to the column's scale, which
    // is why the service refuses a five-decimal input before the statement
    // runs (`validate_money`): the round trip would otherwise not be exact.
    let rounded = insert_observation(
        &pool,
        &NewObservation {
            user_id: owner,
            listing_id: listing,
            amount: "1499.87654",
            fingerprint: "money-3",
            run_id: None,
        },
    )
    .await
    .expect("a value with five decimals must be accepted and rounded");
    let rounded_amount: String =
        sqlx::query_scalar("SELECT amount::text FROM price_observations WHERE id = $1")
            .bind(rounded)
            .fetch_one(&pool)
            .await
            .expect("the rounded observation must be readable");
    assert_eq!(
        rounded_amount, "1499.8765",
        "rounded away from zero at scale 4"
    );
}

// ---------------------------------------------------------------------------
// The composite keys: a child cannot claim one owner and belong to another
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_listing_cannot_belong_to_another_owners_product() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "mon-listing-owner").await;
    let other = insert_account(&pool, "mon-listing-other").await;

    let product = insert_product(&pool, owner, "X1").await;

    // The listing is honest about its own owner and lies about the product's:
    // the direct `users` key is satisfied, so only the composite key can
    // reject it.
    let cross_owner = insert_listing(
        &pool,
        &NewListing {
            user_id: other,
            product_id: product,
            ..NewListing::new(other, product, "https://fixture.test/cross-listing")
        },
    )
    .await
    .map(|_| ());
    assert_rejected(cross_owner, "23503", "product_listings_product_fk");

    // The same row with the product's owner is accepted, so the rejection
    // above is not caused by something else in the row.
    insert_listing(
        &pool,
        &NewListing::new(owner, product, "https://fixture.test/cross-listing"),
    )
    .await
    .expect("a listing owned by the product's owner must be accepted");
}

#[tokio::test]
async fn an_observation_cannot_belong_to_another_owners_listing() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "mon-obs-owner").await;
    let other = insert_account(&pool, "mon-obs-other").await;

    let product = insert_product(&pool, owner, "X1").await;
    let listing = insert_listing(
        &pool,
        &NewListing::new(owner, product, "https://fixture.test/cross-obs"),
    )
    .await
    .expect("the fixture listing must be inserted");

    let cross_owner = insert_observation(
        &pool,
        &NewObservation {
            user_id: other,
            listing_id: listing,
            amount: "10.0000",
            fingerprint: "cross-obs",
            run_id: None,
        },
    )
    .await
    .map(|_| ());
    assert_rejected(cross_owner, "23503", "price_observations_listing_fk");

    insert_observation(
        &pool,
        &NewObservation {
            user_id: owner,
            listing_id: listing,
            amount: "10.0000",
            fingerprint: "cross-obs",
            run_id: None,
        },
    )
    .await
    .expect("an observation owned by the listing's owner must be accepted");
}

#[tokio::test]
async fn an_observation_cannot_name_another_owners_run() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "mon-run-owner").await;
    let other = insert_account(&pool, "mon-run-other").await;

    let product = insert_product(&pool, owner, "X1").await;
    let listing = insert_listing(
        &pool,
        &NewListing::new(owner, product, "https://fixture.test/cross-run"),
    )
    .await
    .expect("the fixture listing must be inserted");
    let foreign_run = insert_run(&pool, other).await;

    let cross_owner = insert_observation(
        &pool,
        &NewObservation {
            user_id: owner,
            listing_id: listing,
            amount: "10.0000",
            fingerprint: "cross-run",
            run_id: Some(foreign_run),
        },
    )
    .await
    .map(|_| ());
    assert_rejected(cross_owner, "23503", "price_observations_run_fk");
}

#[tokio::test]
async fn an_alert_cannot_name_another_owners_rule_or_observation() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "mon-alert-owner").await;
    let other = insert_account(&pool, "mon-alert-other").await;

    let product = insert_product(&pool, owner, "X1").await;
    let listing = insert_listing(
        &pool,
        &NewListing::new(owner, product, "https://fixture.test/cross-alert"),
    )
    .await
    .expect("the fixture listing must be inserted");
    let rule = insert_rule(&pool, owner, product, "500.0000", None)
        .await
        .expect("the fixture rule must be inserted");
    let observation = insert_observation(
        &pool,
        &NewObservation {
            user_id: owner,
            listing_id: listing,
            amount: "499.0000",
            fingerprint: "cross-alert",
            run_id: None,
        },
    )
    .await
    .expect("the fixture observation must be inserted");

    // The rule belongs to the owner, the alert claims the intruder.
    let foreign_rule = insert_alert(&pool, other, rule, observation)
        .await
        .map(|_| ());
    assert_rejected(foreign_rule, "23503", "price_alerts_rule_fk");

    // The observation belongs to the owner, the alert claims the intruder.
    let foreign_observation = insert_alert(
        &pool,
        other,
        {
            insert_rule(
                &pool,
                other,
                insert_product(&pool, other, "XPS").await,
                "500.0000",
                None,
            )
            .await
            .expect("the intruder's fixture rule must be inserted")
        },
        observation,
    )
    .await
    .map(|_| ());
    assert_rejected(foreign_observation, "23503", "price_alerts_observation_fk");
}

// ---------------------------------------------------------------------------
// Deletion behaviour
// ---------------------------------------------------------------------------

#[tokio::test]
async fn deleting_a_product_cascades_to_every_row_under_it() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "mon-product-cascade").await;

    let product = insert_product(&pool, owner, "X1").await;
    let listing = insert_listing(
        &pool,
        &NewListing::new(owner, product, "https://fixture.test/cascade"),
    )
    .await
    .expect("the fixture listing must be inserted");
    let rule = insert_rule(&pool, owner, product, "500.0000", None)
        .await
        .expect("the fixture rule must be inserted");
    let observation = insert_observation(
        &pool,
        &NewObservation {
            user_id: owner,
            listing_id: listing,
            amount: "499.0000",
            fingerprint: "cascade",
            run_id: None,
        },
    )
    .await
    .expect("the fixture observation must be inserted");
    insert_alert(&pool, owner, rule, observation)
        .await
        .expect("the fixture alert must be inserted");

    sqlx::query("DELETE FROM tracked_products WHERE id = $1 AND user_id = $2")
        .bind(product)
        .bind(owner)
        .execute(&pool)
        .await
        .expect("the product must be deletable");

    for (table, column, value) in [
        ("product_listings", "product_id", product),
        ("price_observations", "listing_id", listing),
        ("monitoring_rules", "product_id", product),
    ] {
        let rows: i64 =
            sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE {column} = $1"))
                .bind(value)
                .fetch_one(&pool)
                .await
                .unwrap_or_else(|error| {
                    panic!("{table} must be countable after the cascade: {error}")
                });
        assert_eq!(rows, 0, "deleting the product must delete its {table}");
    }

    let alerts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM price_alerts WHERE user_id = $1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .expect("the alerts must be countable after the cascade");
    assert_eq!(
        alerts, 0,
        "the alert went with the rule and the observation, in the same statement"
    );
}

#[tokio::test]
async fn deleting_a_run_is_refused_while_its_observations_remain() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "mon-run-no-action").await;

    let product = insert_product(&pool, owner, "X1").await;
    let listing = insert_listing(
        &pool,
        &NewListing::new(owner, product, "https://fixture.test/run-no-action"),
    )
    .await
    .expect("the fixture listing must be inserted");
    let run = insert_run(&pool, owner).await;
    insert_observation(
        &pool,
        &NewObservation {
            user_id: owner,
            listing_id: listing,
            amount: "10.0000",
            fingerprint: "run-no-action",
            run_id: Some(run),
        },
    )
    .await
    .expect("the fixture observation must be inserted");

    // `NO ACTION`, not `CASCADE`: the run is audit data, and the observation
    // keeps pointing at it until the account is deleted. The API publishes no
    // delete for a run at all; this is the database half of that decision.
    let refused = sqlx::query("DELETE FROM automation_runs WHERE id = $1")
        .bind(run)
        .execute(&pool)
        .await
        .map(|_| ());
    assert_rejected(refused, "23503", "price_observations_run_fk");

    // Deleting the **observation** first frees the run.
    sqlx::query("DELETE FROM price_observations WHERE run_id = $1")
        .bind(run)
        .execute(&pool)
        .await
        .expect("the observation must be deletable");

    sqlx::query("DELETE FROM automation_runs WHERE id = $1")
        .bind(run)
        .execute(&pool)
        .await
        .expect("the run must be deletable once nothing references it");
}

#[tokio::test]
async fn deleting_an_account_deletes_its_monitoring_rows_and_spares_the_bystander() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "mon-account-cascade").await;
    let bystander = insert_account(&pool, "mon-account-bystander").await;

    let owner_product = insert_product(&pool, owner, "X1").await;
    let owner_listing = insert_listing(
        &pool,
        &NewListing::new(owner, owner_product, "https://fixture.test/account-owner"),
    )
    .await
    .expect("the owner's fixture listing must be inserted");
    let owner_rule = insert_rule(&pool, owner, owner_product, "500.0000", None)
        .await
        .expect("the owner's fixture rule must be inserted");
    let owner_run = insert_run(&pool, owner).await;
    let owner_observation = insert_observation(
        &pool,
        &NewObservation {
            user_id: owner,
            listing_id: owner_listing,
            amount: "499.0000",
            fingerprint: "account-cascade",
            run_id: Some(owner_run),
        },
    )
    .await
    .expect("the owner's fixture observation must be inserted");
    insert_alert(&pool, owner, owner_rule, owner_observation)
        .await
        .expect("the owner's fixture alert must be inserted");

    // A second account's rows must survive: the cascade follows the owner,
    // not the table.
    let kept_product = insert_product(&pool, bystander, "XPS").await;
    let kept_listing = insert_listing(
        &pool,
        &NewListing::new(
            bystander,
            kept_product,
            "https://fixture.test/account-bystander",
        ),
    )
    .await
    .expect("the bystander's fixture listing must be inserted");

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(owner)
        .execute(&pool)
        .await
        .expect("the account must be deletable");

    for (table, extra) in [
        ("tracked_products", ""),
        ("product_listings", ""),
        ("price_observations", ""),
        ("monitoring_rules", ""),
        ("automation_runs", ""),
        ("price_alerts", ""),
    ] {
        let rows: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {table} WHERE user_id = $1 {extra}"
        ))
        .bind(owner)
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|error| panic!("{table} must be countable after the cascade: {error}"));
        assert_eq!(rows, 0, "deleting the account must delete its {table}");
    }

    let kept: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tracked_products WHERE user_id = $1")
        .bind(bystander)
        .fetch_one(&pool)
        .await
        .expect("the bystander's products must be countable");
    assert_eq!(kept, 1, "another account's product must survive");

    let kept_listings: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM product_listings WHERE id = $1 AND user_id = $2")
            .bind(kept_listing)
            .bind(bystander)
            .fetch_one(&pool)
            .await
            .expect("the bystander's listing must be countable");
    assert_eq!(kept_listings, 1, "another account's listing must survive");

    // The bystander is not needed after the assertions; removing it keeps the
    // shared database tidy without touching anything this test did not create.
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(bystander)
        .execute(&pool)
        .await
        .expect("the bystander account must be removable");
}

// ---------------------------------------------------------------------------
// The URL and the one-rule-per-product limit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_listing_url_is_bounded_and_scheme_checked_by_the_database() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "mon-url").await;
    let product = insert_product(&pool, owner, "X1").await;

    let refused = insert_listing(
        &pool,
        &NewListing {
            url: "javascript:alert(1)",
            ..NewListing::new(owner, product, "https://fixture.test/unused")
        },
    )
    .await
    .map(|_| ());
    assert_rejected(refused, "23514", "product_listings_url_chk");

    let over_long = insert_listing(
        &pool,
        &NewListing {
            url: &format!("https://fixture.test/{}", "a".repeat(2_048)),
            ..NewListing::new(owner, product, "https://fixture.test/unused")
        },
    )
    .await
    .map(|_| ());
    assert_rejected(over_long, "23514", "product_listings_url_chk");

    // A URL at the bound is accepted.
    insert_listing(
        &pool,
        &NewListing {
            url: &format!("https://fixture.test/{}", "a".repeat(2_022)),
            ..NewListing::new(owner, product, "https://fixture.test/unused")
        },
    )
    .await
    .expect("a URL at the bound must be accepted");
}

#[tokio::test]
async fn a_product_has_at_most_one_rule() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "mon-one-rule").await;

    let product = insert_product(&pool, owner, "X1").await;
    insert_rule(&pool, owner, product, "500.0000", None)
        .await
        .expect("the first rule must be accepted");

    let second = insert_rule(&pool, owner, product, "900.0000", Some(10))
        .await
        .map(|_| ());
    assert_rejected(
        second,
        "23505",
        "monitoring_rules_user_id_product_unique_idx",
    );

    // Another product of the same owner carries its own rule.
    let other_product = insert_product(&pool, owner, "XPS").await;
    insert_rule(&pool, owner, other_product, "900.0000", Some(10))
        .await
        .expect("a second product's rule is a different row");
}
