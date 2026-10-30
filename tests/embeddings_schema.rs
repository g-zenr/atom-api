//! Integration tests for the embedding-index schema (`migrations/0006`).
//!
//! `0006` adds one table and no endpoint of its own, so most of the contract
//! lives in the database: the `CHECK`s, the unique index the re-embed upsert
//! targets and the `ON DELETE` action are properties of PostgreSQL, not of
//! this crate, and the only way to prove them is to speak SQL to a real,
//! disposable database. That is the approach `claims_schema.rs` takes, and the
//! scratch-database helpers here are its pattern: a database named
//! `atom_test_scratch_<uuid>`, migrated from nothing, dropped again — with a
//! `Drop` backstop for a panicking assertion.
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

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};
use uuid::Uuid;

/// The embedder identifier of every fixture row. Length is a separate
/// constraint with its own migration rule; here it only has to be valid.
const MODEL_ID: &str = "keys-schema-test-embedder-v1";

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
    .bind("embeddings-schema-fixture-not-a-real-hash")
    .bind("Embeddings Schema Fixture")
    .execute(pool)
    .await
    .expect("the fixture account must be inserted");

    id
}

/// Inserts one embedding and returns its id, or the error the server returned.
///
/// `dim` is passed explicitly rather than derived from `vector.len()`: the
/// whole point of the dimension constraint is that the two can disagree, and
/// the database has to reject the disagreement.
async fn insert_embedding(
    pool: &PgPool,
    user_id: Uuid,
    source_type: &str,
    source_id: Uuid,
    model_id: &str,
    dim: i32,
    vector: Vec<f32>,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO embeddings (id, user_id, source_type, source_id, model_id, dim, vector)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(user_id)
    .bind(source_type)
    .bind(source_id)
    .bind(model_id)
    .bind(dim)
    .bind(vector)
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
    embeddings_table: bool,
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

    let embeddings_table: bool = sqlx::query_scalar("SELECT to_regclass('embeddings') IS NOT NULL")
        .fetch_one(&mut connection)
        .await
        .map_err(|error| format!("the new table must be probeable: {error}"))?;

    Ok(ScratchProbe {
        tables_before,
        versions,
        embeddings_table,
    })
}

// ---------------------------------------------------------------------------
// The migration applies from nothing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_embeddings_table_applies_from_an_empty_database() {
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
         including 0006"
    );
    assert!(
        probe.embeddings_table,
        "`embeddings` must exist after the migrations ran"
    );
}

// ---------------------------------------------------------------------------
// The source-type discriminator
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_embedding_rejects_a_source_type_that_is_not_note_or_application() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "embeddings-source-type").await;

    // Both accepted values first, so the test cannot pass against a table that
    // refuses every source type, and each must round-trip unchanged.
    let note = insert_embedding(
        &pool,
        owner,
        "note",
        Uuid::new_v4(),
        MODEL_ID,
        4,
        vec![0.25; 4],
    )
    .await
    .expect("a `note` source must be accepted");
    let application = insert_embedding(
        &pool,
        owner,
        "application",
        Uuid::new_v4(),
        MODEL_ID,
        2,
        vec![-1.0, 1.0],
    )
    .await
    .expect("an `application` source must be accepted");

    let stored_note: String =
        sqlx::query_scalar("SELECT source_type FROM embeddings WHERE id = $1")
            .bind(note)
            .fetch_one(&pool)
            .await
            .expect("the note embedding must be readable");
    let stored_application: String =
        sqlx::query_scalar("SELECT source_type FROM embeddings WHERE id = $1")
            .bind(application)
            .fetch_one(&pool)
            .await
            .expect("the application embedding must be readable");
    assert_eq!(stored_note, "note", "the source type must round-trip");
    assert_eq!(
        stored_application, "application",
        "the source type must round-trip"
    );

    // The CHECK is a two-value discriminator: the query builder matches on it
    // before it chooses a static statement, so any other value — including a
    // case variant or a padded one — must be unrepresentable.
    for source_type in ["document", "NOTE", "Application", "", "note ", "notes"] {
        let result = insert_embedding(
            &pool,
            owner,
            source_type,
            Uuid::new_v4(),
            MODEL_ID,
            1,
            vec![0.5],
        )
        .await
        .map(|_| ());

        assert_rejected(result, "23514", "embeddings_source_type_chk");
    }

    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM embeddings WHERE user_id = $1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .expect("the owner's embeddings must be countable");
    assert_eq!(stored, 2, "a rejected insert must leave no row behind");
}

// ---------------------------------------------------------------------------
// The dimension constraint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_embedding_rejects_a_dimension_that_disagrees_with_the_vector() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "embeddings-dimension").await;

    // The consistent shape first: `dim` is the declared length of the vector,
    // stored so a reader can reject a candidate set from another model without
    // inspecting every array.
    insert_embedding(
        &pool,
        owner,
        "note",
        Uuid::new_v4(),
        MODEL_ID,
        4,
        vec![0.5, -0.5, 0.25, 0.75],
    )
    .await
    .expect("a vector whose length matches dim must be accepted");

    // Every way the declaration and the stored array can disagree. The empty
    // vector with `dim = 0` fails the `dim > 0` arm — `cardinality` returns 0
    // for an empty array, which is why the migration uses it rather than
    // `array_length`, whose NULL would let a CHECK pass vacuously.
    let inconsistent: [(i32, Vec<f32>); 4] = [
        (3, vec![0.5, 0.5, 0.5, 0.5]), // declared short, stored long
        (5, vec![0.5, 0.5, 0.5, 0.5]), // declared long, stored short
        (0, vec![]),                   // nothing stored, nothing declared
        (-1, vec![0.5]),               // a dimension is never negative
    ];
    for (dim, vector) in inconsistent {
        let result = insert_embedding(&pool, owner, "note", Uuid::new_v4(), MODEL_ID, dim, vector)
            .await
            .map(|_| ());

        assert_rejected(result, "23514", "embeddings_dimension_chk");
    }

    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM embeddings WHERE user_id = $1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .expect("the owner's embeddings must be countable");
    assert_eq!(stored, 1, "a rejected insert must leave no row behind");
}

// ---------------------------------------------------------------------------
// The upsert target
// ---------------------------------------------------------------------------

#[tokio::test]
async fn one_source_cannot_hold_two_embeddings_for_the_same_model() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "embeddings-unique").await;
    let other = insert_account(&pool, "embeddings-unique-other").await;
    let source = Uuid::new_v4();

    insert_embedding(&pool, owner, "note", source, MODEL_ID, 2, vec![0.5, 0.5])
        .await
        .expect("the first embedding must be accepted");

    // The exact tuple the re-embed upsert names as its conflict target: a
    // second row for the same owner, source and model is the collision the
    // index exists to turn into a `23505`, not into a check-then-insert race
    // two concurrent reindexes could both win.
    let duplicate = insert_embedding(&pool, owner, "note", source, MODEL_ID, 2, vec![-0.5, -0.5])
        .await
        .map(|_| ());
    assert_rejected(
        duplicate,
        "23505",
        "embeddings_user_id_source_model_unique_idx",
    );

    // A different model is a different meaning of the vector space, and a new
    // row: the conflict target is the model id too, so this must be accepted.
    insert_embedding(
        &pool,
        owner,
        "note",
        source,
        "other-model-v2",
        2,
        vec![1.0, 0.0],
    )
    .await
    .expect("a second model on the same source must be accepted");

    // The index leads with `user_id`, so another owner's embedding of the same
    // source id is a row of their own, not a conflict.
    insert_embedding(&pool, other, "note", source, MODEL_ID, 2, vec![0.0, 1.0])
        .await
        .expect("another owner's embedding of the same source must be accepted");

    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM embeddings WHERE user_id = $1")
        .bind(owner)
        .fetch_one(&pool)
        .await
        .expect("the owner's embeddings must be countable");
    assert_eq!(
        stored, 2,
        "the duplicate must not have been stored; the two distinct rows must have been"
    );
}

// ---------------------------------------------------------------------------
// Deletion behaviour
// ---------------------------------------------------------------------------

#[tokio::test]
async fn deleting_an_account_deletes_its_embeddings_and_keeps_a_bystanders() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "embeddings-cascade").await;
    let bystander = insert_account(&pool, "embeddings-bystander").await;

    // The table carries the owner in its own column, and the foreign key
    // cascades: an embedding is a derived artefact of the account and has no
    // meaning without it.
    insert_embedding(
        &pool,
        owner,
        "note",
        Uuid::new_v4(),
        MODEL_ID,
        2,
        vec![0.5, 0.5],
    )
    .await
    .expect("the owner's embedding must be inserted");

    // A second account's embedding must survive: the cascade follows the
    // owner, not the table.
    let kept = insert_embedding(
        &pool,
        bystander,
        "note",
        Uuid::new_v4(),
        MODEL_ID,
        2,
        vec![0.5, 0.5],
    )
    .await
    .expect("the bystander's embedding must be inserted");

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(owner)
        .execute(&pool)
        .await
        .expect("the account must be deletable");

    let owner_embeddings: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM embeddings WHERE user_id = $1")
            .bind(owner)
            .fetch_one(&pool)
            .await
            .expect("the deleted owner's embeddings must be countable");
    let kept_embeddings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM embeddings WHERE id = $1")
        .bind(kept)
        .fetch_one(&pool)
        .await
        .expect("the bystander's embedding must be countable");

    assert_eq!(
        owner_embeddings, 0,
        "deleting an account must delete its embeddings"
    );
    assert_eq!(
        kept_embeddings, 1,
        "another account's embedding must survive"
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
// The deliberate absence of a foreign key on `source_id`
// ---------------------------------------------------------------------------

/// A source id that matches no row anywhere is accepted.
///
/// `source_id` is polymorphic — it names a note or an application — and
/// PostgreSQL cannot express a foreign key to one of two tables from one
/// column, so the migration deliberately ships none (the header of
/// `migrations/0006`). The guarantee lives in the write path instead, per
/// ADR-0008 §3: the source row and its embedding are written in the same
/// statement, the source delete purges its own embeddings in that same
/// statement, and the owner-scoped reindex sweep is the backstop. This test
/// pins the absence of the key, so that a well-meaning consistency edit cannot
/// add a reference to one of the two tables and silently break the other.
#[tokio::test]
async fn an_embedding_may_reference_a_source_row_that_does_not_exist() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "embeddings-no-fk").await;

    // A fresh UUID that names no note and no application in this database.
    let missing_source = Uuid::new_v4();

    let id = insert_embedding(
        &pool,
        owner,
        "note",
        missing_source,
        MODEL_ID,
        2,
        vec![0.5, 0.5],
    )
    .await
    .expect(
        "a source_id with no matching row must be accepted: the column carries \
         no foreign key by design",
    );

    let stored: Uuid = sqlx::query_scalar("SELECT source_id FROM embeddings WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("the accepted embedding must be readable");
    assert_eq!(
        stored, missing_source,
        "the source id must be stored exactly as given"
    );
}

// ---------------------------------------------------------------------------
// The vector column
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_vector_round_trips_through_the_real_array_column() {
    let pool = migrated_pool().await;
    let owner = insert_account(&pool, "embeddings-vector").await;

    // The column is `REAL[]` — float4, not double, and not pgvector: the
    // hybrid search re-ranks a bounded candidate set in Rust over the built-in
    // array type (ADR-0008). What is stored must come back bit-identical as
    // Rust `f32` values, or the re-ranking would score against a vector the
    // embedder never produced.
    let vector = vec![0.5, -1.25, 0.1, 3.0];

    let id = insert_embedding(
        &pool,
        owner,
        "note",
        Uuid::new_v4(),
        MODEL_ID,
        4,
        vector.clone(),
    )
    .await
    .expect("the vector must be accepted");

    let stored: Vec<f32> = sqlx::query_scalar("SELECT vector FROM embeddings WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("the stored vector must be readable");

    assert_eq!(stored, vector, "the vector must round-trip unchanged");
}
