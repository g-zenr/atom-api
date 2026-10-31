//! Integration tests for the API-key schema (`migrations/0007`).
//!
//! `0007` adds one table and no endpoint of its own, so most of the contract
//! lives in the database: a `CHECK`, a `UNIQUE` constraint and an `ON DELETE`
//! action are properties of PostgreSQL, not of this crate, and the only way to
//! prove them is to speak SQL to a real, disposable database. That is the same
//! approach `claims_schema.rs` takes, and the scratch-database helpers here are
//! its pattern: a database named `atom_test_scratch_<uuid>`, migrated from
//! nothing, dropped again — with a `Drop` backstop for a panicking assertion.
//!
//! Every rejection asserts the SQLSTATE **and** the constraint name the server
//! reports — "an error happened" is not evidence that the constraint under
//! test is the thing that fired — and every rejection is paired with an
//! accepted control row, so a test cannot pass against a table that refuses
//! everything.
//!
//! The tests are independent. Each fixture is built from a fresh UUID and a
//! unique email address, and each assertion is restricted to the ids the test
//! created, so no test depends on the order another one ran in and none
//! truncates a shared table. The one shared resource is the disposable test
//! database itself, which the harness refuses to use unless its name contains
//! `test`.

pub mod common;

use std::str::FromStr;

use chrono::{DateTime, TimeZone, Utc};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};
use uuid::Uuid;

/// The display prefix of a well-formed key: `atom_` plus eight lower-case hex
/// characters. The grammar itself is the subject of its own test; here it only
/// has to be valid.
const GOOD_PREFIX: &str = "atom_deadbeef";

/// The owner-chosen label of a fixture key. Length is a separate constraint
/// with its own test; here it only has to be valid.
const KEY_NAME: &str = "a fixture key";

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
    .bind("keys-schema-fixture-not-a-real-hash")
    .bind("Keys Schema Fixture")
    .execute(pool)
    .await
    .expect("the fixture account must be inserted");

    id
}

/// Inserts one API key and returns its id, or the error the server returned.
///
/// Only the four values a fixture needs are named: the timestamps take the
/// column defaults, which is exactly what the issuing repository does.
async fn insert_key(
    pool: &PgPool,
    user_id: Uuid,
    name: &str,
    prefix: &str,
    key_hash: &str,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO api_keys (id, user_id, name, prefix, key_hash) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(user_id)
    .bind(name)
    .bind(prefix)
    .bind(key_hash)
    .execute(pool)
    .await?;

    Ok(id)
}

/// Removes a fixture account and, through the cascade, every key it owned.
///
/// The shared test database persists between runs, so a test leaves it as it
/// found it. A row left behind by a run that panicked mid-test is still
/// harmless to the next one, because every accepted fixture carries a
/// [`fresh_digest`].
async fn remove_account(pool: &PgPool, id: Uuid) {
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .expect("the fixture account must be removable");
}

/// A 64-character lower-case hex digest derived from `seed`.
///
/// One byte rendered 32 times: distinct seeds give distinct digests, and the
/// shape is exactly what `api_keys_key_hash_chk` accepts, without the test
/// depending on the crate's own hashing to know what a digest looks like.
/// Used only for **rejected** values: a rejected row is never stored, so a
/// fixed value cannot collide with a previous run of the suite. An accepted
/// fixture takes [`fresh_digest`] instead.
fn digest(seed: u8) -> String {
    format!("{seed:02x}").repeat(32)
}

/// A fresh, well-formed digest, unique to this run.
///
/// The shared test database persists between runs, and the digest is the
/// unique identity of a credential, so an accepted fixture must never carry a
/// fixed value: a second run of the suite would collide with the row a
/// previous one stored. Two UUIDs rendered as simple hex are exactly 64
/// lower-case hex characters.
fn fresh_digest() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// A well-formed prefix that varies with `seed`, so distinct rows can carry
/// distinct display labels without colliding on the value.
fn prefix_with(seed: u8) -> String {
    format!("atom_{:02x}beef01", seed)
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
    api_keys_table: bool,
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

    let api_keys_table: bool = sqlx::query_scalar("SELECT to_regclass('api_keys') IS NOT NULL")
        .fetch_one(&mut connection)
        .await
        .map_err(|error| format!("the new table must be probeable: {error}"))?;

    Ok(ScratchProbe {
        tables_before,
        versions,
        api_keys_table,
    })
}

// ---------------------------------------------------------------------------
// The migration applies from nothing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_api_keys_table_applies_from_an_empty_database() {
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
         including 0007"
    );
    assert!(
        probe.api_keys_table,
        "`api_keys` must exist after the migrations ran"
    );
}

// ---------------------------------------------------------------------------
// The prefix grammar
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_key_rejects_a_prefix_outside_the_atom_plus_eight_hex_grammar() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "keys-prefix").await;

    // The accepted form first, so the test cannot pass against a table that
    // refuses every prefix.
    let accepted = insert_key(&pool, owner, KEY_NAME, GOOD_PREFIX, &fresh_digest())
        .await
        .expect("atom_ plus eight lower-case hex characters must be accepted");
    let stored: String = sqlx::query_scalar("SELECT prefix FROM api_keys WHERE id = $1")
        .bind(accepted)
        .fetch_one(&pool)
        .await
        .expect("the accepted key must be readable");
    assert_eq!(stored, GOOD_PREFIX, "the prefix must round-trip unchanged");

    // The CHECK pins the *form*, so a row the issuing code could never have
    // produced is unrepresentable whatever writes it: the middleware decides
    // "is this bearer token a JWT or an API key?" by the `atom_` prefix, so a
    // stored prefix outside the grammar would be a credential it could not
    // classify.
    for prefix in [
        "atom_deadbeeg",  // a letter outside the hex alphabet
        "atom_DEADBEEF",  // upper case
        "atom_deadbee",   // seven hex characters
        "atom_deadbeef0", // nine
        "key_deadbeef",   // a different prefix
        "deadbeef",       // no prefix at all
        "",               // nothing
        " atom_deadbeef", // a leading space
    ] {
        let result = insert_key(&pool, owner, KEY_NAME, prefix, &digest(0x2a))
            .await
            .map(|_| ());

        assert_rejected(result, "23514", "api_keys_prefix_chk");
    }

    let keys: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_keys WHERE user_id = $1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .expect("the owner's keys must be countable");
    assert_eq!(keys, 1, "a rejected insert must leave no row behind");

    remove_account(&pool, owner).await;
}

// ---------------------------------------------------------------------------
// The digest shape
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_key_rejects_a_digest_that_is_not_sixty_four_lower_case_hex_characters() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "keys-digest").await;

    // The accepted form first: the SHA-256 hex digest of a well-formed secret.
    let expected = fresh_digest();
    let accepted = insert_key(&pool, owner, KEY_NAME, GOOD_PREFIX, &expected)
        .await
        .expect("a 64-character lower-case hex digest must be accepted");
    let stored: String = sqlx::query_scalar("SELECT key_hash FROM api_keys WHERE id = $1")
        .bind(accepted)
        .fetch_one(&pool)
        .await
        .expect("the accepted key must be readable");
    assert_eq!(stored, expected, "the digest must round-trip unchanged");

    let mut upper_case = digest(0x3c);
    upper_case.make_ascii_uppercase();
    let mut with_a_non_hex_character = digest(0x3d);
    with_a_non_hex_character.replace_range(0..1, "g");

    for key_hash in [
        digest(0x3b)[..63].to_string(), // one character short
        format!("{}0", digest(0x3e)),   // 65 characters
        upper_case,                     // upper case
        with_a_non_hex_character,       // not the hex alphabet
        String::new(),                  // nothing
    ] {
        let result = insert_key(&pool, owner, KEY_NAME, GOOD_PREFIX, &key_hash)
            .await
            .map(|_| ());

        assert_rejected(result, "23514", "api_keys_key_hash_chk");
    }

    let keys: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_keys WHERE user_id = $1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .expect("the owner's keys must be countable");
    assert_eq!(keys, 1, "a rejected insert must leave no row behind");

    remove_account(&pool, owner).await;
}

// ---------------------------------------------------------------------------
// One row per secret
// ---------------------------------------------------------------------------

#[tokio::test]
async fn two_keys_cannot_carry_the_same_digest() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "keys-unique").await;
    let other = insert_account(&pool, "keys-unique-other").await;

    // One digest shared by every attempt below: the duplicate the constraint
    // must refuse, whatever owner presents it.
    let shared = fresh_digest();

    insert_key(&pool, owner, KEY_NAME, GOOD_PREFIX, &shared)
        .await
        .expect("the first key must be accepted");

    // The same digest under a different owner: a digest is the identity of one
    // credential globally, and a second row holding it is the same secret
    // stored twice — a lookup would have two rows to find and no way to prefer
    // one, so the insert must fail rather than leave the outcome of the
    // authentication seek dependent on which row the planner reads.
    let duplicate_other_owner = insert_key(&pool, other, KEY_NAME, &prefix_with(0x4b), &shared)
        .await
        .map(|_| ());
    assert_rejected(duplicate_other_owner, "23505", "api_keys_key_hash_key");

    // The same digest under the same owner, which is the case the issuing code
    // could actually produce by storing a generated secret twice.
    let duplicate_same_owner = insert_key(&pool, owner, KEY_NAME, &prefix_with(0x4c), &shared)
        .await
        .map(|_| ());
    assert_rejected(duplicate_same_owner, "23505", "api_keys_key_hash_key");

    // A different digest is a different credential and is accepted, so the
    // rejections above are not caused by something else in the row.
    insert_key(&pool, owner, KEY_NAME, &prefix_with(0x4d), &fresh_digest())
        .await
        .expect("a second key with its own digest must be accepted");

    // The accepted rows are exactly the two legitimate ones: the first key
    // and the control row.
    let keys: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_keys WHERE user_id = $1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .expect("the owner's keys must be countable");
    assert_eq!(keys, 2, "only the two legitimate rows may exist");

    remove_account(&pool, owner).await;
    remove_account(&pool, other).await;
}

// ---------------------------------------------------------------------------
// The name bounds
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_name_must_have_between_one_and_one_hundred_characters() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "keys-name").await;

    // The bounds are inclusive: one character and one hundred are both legal.
    let single = insert_key(&pool, owner, "x", GOOD_PREFIX, &fresh_digest())
        .await
        .expect("a one-character name must be accepted");
    let hundred = insert_key(
        &pool,
        owner,
        &"n".repeat(100),
        &prefix_with(0x5b),
        &fresh_digest(),
    )
    .await
    .expect("a one-hundred-character name must be accepted");

    let single_length: i32 =
        sqlx::query_scalar("SELECT CHAR_LENGTH(name) FROM api_keys WHERE id = $1")
            .bind(single)
            .fetch_one(&pool)
            .await
            .expect("the short name must be readable");
    let hundred_length: i32 =
        sqlx::query_scalar("SELECT CHAR_LENGTH(name) FROM api_keys WHERE id = $1")
            .bind(hundred)
            .fetch_one(&pool)
            .await
            .expect("the long name must be readable");
    assert_eq!(single_length, 1, "the short bound must round-trip");
    assert_eq!(hundred_length, 100, "the long bound must round-trip");

    // Both rejections are the name-length constraint: nothing else in the row
    // differs from an accepted one.
    let empty = insert_key(&pool, owner, "", GOOD_PREFIX, &digest(0x5c))
        .await
        .map(|_| ());
    assert_rejected(empty, "23514", "api_keys_name_length_chk");

    let one_past_the_bound = insert_key(
        &pool,
        owner,
        &"n".repeat(101),
        &prefix_with(0x5d),
        &digest(0x5d),
    )
    .await
    .map(|_| ());
    assert_rejected(one_past_the_bound, "23514", "api_keys_name_length_chk");

    let keys: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_keys WHERE user_id = $1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .expect("the owner's keys must be countable");
    assert_eq!(keys, 2, "the rejected names must have left no row behind");

    remove_account(&pool, owner).await;
}

// ---------------------------------------------------------------------------
// The timestamp columns
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_timestamp_columns_take_their_documented_defaults() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "keys-defaults").await;

    // No timestamp is passed — exactly what the issuing repository does.
    // `created_at` must take the column default; `revoked_at` and
    // `last_used_at` must start NULL, because `revoked_at IS NULL` *is*
    // "active" and there is no second representation of the fact to drift
    // from it.
    let active = insert_key(&pool, owner, KEY_NAME, GOOD_PREFIX, &fresh_digest())
        .await
        .expect("a key must be accepted without any timestamp");

    let defaulted: bool = sqlx::query_scalar(
        "SELECT created_at >= clock_timestamp() - INTERVAL '5 minutes'
             AND created_at <= clock_timestamp()
             AND revoked_at IS NULL
             AND last_used_at IS NULL
         FROM api_keys WHERE id = $1",
    )
    .bind(active)
    .fetch_one(&pool)
    .await
    .expect("the stored key must be readable");
    assert!(
        defaulted,
        "created_at must be defaulted to now; revoked_at and last_used_at must be NULL"
    );

    // Revocation is a plain timestamp: setting one stores it unchanged. The
    // first-value-wins rule is the repository's `COALESCE`; here only the
    // storage is pinned.
    let revoked_at = Utc
        .with_ymd_and_hms(2026, 6, 1, 12, 0, 0)
        .single()
        .expect("the fixture timestamp must be valid");
    let revoked = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO api_keys (id, user_id, name, prefix, key_hash, revoked_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(revoked)
    .bind(owner)
    .bind(KEY_NAME)
    .bind(prefix_with(0x6b))
    .bind(fresh_digest())
    .bind(revoked_at)
    .execute(&pool)
    .await
    .expect("a key with an explicit revocation instant must be accepted");

    let stored: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT revoked_at FROM api_keys WHERE id = $1")
            .bind(revoked)
            .fetch_one(&pool)
            .await
            .expect("the revoked key must be readable");
    assert_eq!(
        stored,
        Some(revoked_at),
        "an explicit revoked_at must be stored unchanged"
    );

    remove_account(&pool, owner).await;
}

// ---------------------------------------------------------------------------
// Deletion behaviour
// ---------------------------------------------------------------------------

#[tokio::test]
async fn deleting_an_account_deletes_its_keys_and_keeps_a_bystanders() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "keys-cascade").await;
    let bystander = insert_account(&pool, "keys-bystander").await;

    // A key is an authority derived from the account and has no meaning
    // without it, so the foreign key cascades the delete.
    insert_key(&pool, owner, KEY_NAME, &prefix_with(0x7a), &fresh_digest())
        .await
        .expect("the owner's first key must be inserted");
    insert_key(&pool, owner, KEY_NAME, &prefix_with(0x7b), &fresh_digest())
        .await
        .expect("the owner's second key must be inserted");

    // A second account's key must survive: the cascade follows the owner, not
    // the table.
    let kept = insert_key(
        &pool,
        bystander,
        KEY_NAME,
        &prefix_with(0x7c),
        &fresh_digest(),
    )
    .await
    .expect("the bystander's key must be inserted");

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(owner)
        .execute(&pool)
        .await
        .expect("the account must be deletable");

    let owner_keys: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_keys WHERE user_id = $1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .expect("the deleted owner's keys must be countable");
    let kept_keys: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_keys WHERE id = $1")
        .bind(kept)
        .fetch_one(&pool)
        .await
        .expect("the bystander's key must be countable");

    assert_eq!(owner_keys, 0, "deleting an account must delete its keys");
    assert_eq!(kept_keys, 1, "another account's key must survive");

    remove_account(&pool, bystander).await;
}

// ---------------------------------------------------------------------------
// The deliberate absence of `updated_at`
// ---------------------------------------------------------------------------

#[tokio::test]
async fn api_keys_has_no_updated_at_column() {
    let pool = migrated_pool().await;

    // The migration's header records the design: the row is immutable apart
    // from `revoked_at` and `last_used_at`, and a column the hourly usage
    // touch would rewrite carries no information the touch itself does not.
    // Pinning the exact column set means a later migration cannot add the
    // column — or lose a documented one — without this test naming it.
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns
         WHERE table_schema = 'public' AND table_name = 'api_keys'
         ORDER BY ordinal_position",
    )
    .fetch_all(&pool)
    .await
    .expect("the column list must be readable");

    assert_eq!(
        columns,
        vec![
            "id",
            "user_id",
            "name",
            "prefix",
            "key_hash",
            "last_used_at",
            "created_at",
            "revoked_at",
        ],
        "the api_keys columns must be exactly the documented set, with no updated_at"
    );
}
