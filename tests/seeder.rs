//! The development seeder (`atom-seed`): guards, determinism and idempotency.
//!
//! Every test drives the real binary through [`std::process::Command`], because
//! the guards, the argument parser and the output are properties of the process
//! and not of a library function. The reported command is
//! [`env!("CARGO_BIN_EXE_atom-seed")`], which Cargo builds next to the test
//! binary.
//!
//! **The child environment is set explicitly.** Each run gets `DATABASE_URL`
//! and `APP_ENV` from the test, never from the developer's shell, and a test
//! that needs a variable to be absent removes it from the child. The repository
//! has no `.env`, so `dotenvy` in the child cannot add one back.
//!
//! **The database is the shared, disposable `atom_test`.** It is never empty —
//! every other integration test leaves an account behind — so every run that is
//! expected to reach the foreign-account guard passes `--allow-non-empty`. The
//! assertions are restricted to the two seeded ids, never to a table-wide
//! `COUNT(*)`.
//!
//! **The one exception is the ordering test.** `A-C-S1-7` says the `G3`
//! preflight runs before `sqlx::migrate!`, and the shared database cannot show
//! that: it is migrated before the test starts, so "no migration was applied"
//! is true for every run that ever happens there. That test therefore creates a
//! database of its own with a hand-made `users` table and no `_sqlx_migrations`,
//! asserts that the seeder refuses, and asserts that the table is *still*
//! absent afterwards — a statement only a run that refused before migrating can
//! leave true.
//!
//! **The tests are serialised, within this binary and against a second copy of
//! the suite.** They share one deterministic dataset in one database, and one
//! of them deliberately makes an insert fail. Two seeder processes overlapping
//! would make the observable result depend on which one wins the race, so a
//! test holds a thread mutex and a PostgreSQL session advisory lock for its
//! whole run. That is a lock, not shared state: every test still builds and
//! asserts its own dataset, and no test depends on the order another one ran
//! in.
//!
//! The contract is `C-S1` in `docs/engineering/runs/chain/ledger.md`, as
//! amended by `A-C-S1-5` … `A-C-S1-10`, with the detail in
//! `docs/database/seeding.md`.

pub mod common;

use std::process::Command;
use std::str::FromStr;

use argon2::{Argon2, PasswordHash, PasswordVerifier};
use chrono::{DateTime, Utc};
use sqlx::Connection;
use sqlx::PgConnection;
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use uuid::Uuid;

/// The binary under test.
const SEEDER: &str = env!("CARGO_BIN_EXE_atom-seed");

/// The two accounts the seeder publishes (seeding.md §8).
const SEEDED_EMAILS: [&str; 2] = ["owner@atom.local", "other@atom.local"];

/// The published development password.
const SEEDED_PASSWORD: &str = "atom-dev-password-1234";

/// The `dev` profile counts (seeding.md §2).
const DEV_USERS: i64 = 2;
const DEV_NOTES: i64 = 200;
const DEV_PROJECTS: i64 = 8;
const DEV_TASKS: i64 = 60;
/// The `dev` profile's expense claims and line items (`C-R1` §SEEDER).
///
/// The item count is derived from the claim count: `1 + (i % 3)` items for
/// every claim except the first, which is a draft and carries none.
const DEV_CLAIMS: i64 = 25;
const DEV_CLAIM_ITEMS: i64 = 48;

/// The `dev` profile's job applications and the two counts derived from them
/// (`C-J1` §SEEDER, `seeding.md` §2 and §7.4).
///
/// The derivation, recomputed rather than copied: 20 applications alternate
/// between the two owners (`i % 2`), so each owner has 10; each owner gets
/// `ceil(10 / 3) + 2 == 4 + 2 == 6` companies, so 12 in total; and each
/// application `i` carries `1 + (i % 4)` events, which over five cycles of
/// `0..3` is `5 × (1 + 2 + 3 + 4) == 50`.
const DEV_APPLICATIONS: i64 = 20;
const DEV_COMPANIES: i64 = 12;
const DEV_EVENTS: i64 = 50;

/// The `minimal` profile counts (seeding.md §2).
const MINIMAL_USERS: i64 = 2;
const MINIMAL_NOTES: i64 = 5;
const MINIMAL_PROJECTS: i64 = 1;
const MINIMAL_TASKS: i64 = 3;

/// A loopback port where nothing listens, on a database name `G2` admits.
///
/// A run against it can only produce a connection failure, so a message that
/// names a guard proves the guard ran before the connection.
const UNREACHABLE_URL: &str = "postgres://atom:atom@127.0.0.1:1/atom_test";

/// Fragments that must never appear in any seeder output (G4, A-C-S1-8).
const NEVER_PRINTED: [&str; 4] = ["postgres://", "atom:atom", "127.0.0.1", "$argon2"];

/// Serialises the tests of this binary. See the module documentation.
static SEEDER_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The advisory-lock key every copy of this suite uses.
///
/// The bytes spell `ATOMSEED`, so two copies of the suite cannot pick a
/// different key by accident and the value cannot collide with the key
/// `sqlx::migrate!` takes for its own migration lock.
const SEEDER_LOCK_KEY: i64 = 0x4154_4f4d_5345_4544;

/// Holds both seeder locks for the duration of a test.
///
/// The advisory lock lives on a session of its own, so PostgreSQL releases it
/// when that connection closes — a crashed test cannot leave the suite wedged.
struct SeederLock {
    /// Declared first so it is dropped first: the server releases the advisory
    /// lock before the next thread of this process takes the mutex.
    _session: sqlx::PgConnection,
    _thread: tokio::sync::MutexGuard<'static, ()>,
}

/// Takes the thread lock and then the cross-process lock.
///
/// The order is the same in every test, so two copies of the suite cannot
/// deadlock against each other.
async fn seeder_lock() -> SeederLock {
    let thread = SEEDER_LOCK.lock().await;

    let url = common::test_database_url();
    let mut session = sqlx::PgConnection::connect(&url)
        .await
        .expect("the test database must be reachable for the seeder lock");

    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(SEEDER_LOCK_KEY)
        .execute(&mut session)
        .await
        .expect("the seeder advisory lock must be acquirable");

    SeederLock {
        _session: session,
        _thread: thread,
    }
}

/// One finished seeder process.
#[derive(Debug)]
struct SeedRun {
    /// The exit code, or `None` when the process was killed by a signal.
    status: Option<i32>,
    stdout: String,
    stderr: String,
}

impl SeedRun {
    fn succeeded(&self) -> bool {
        self.status == Some(0)
    }

    /// Both streams, because a refusal may be written to either.
    fn output(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }

    /// Asserts that the run succeeded and returns its output.
    fn require_success(&self, what: &str) -> String {
        assert!(
            self.succeeded(),
            "{what}: the seeder must succeed, exit status {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            self.status,
            self.stdout,
            self.stderr
        );
        self.output()
    }

    /// Asserts that the run refused and returns its output.
    fn require_refusal(&self, what: &str) -> String {
        let output = self.output();
        assert!(
            self.status.is_some(),
            "{what}: the seeder must exit, not be killed by a signal\n{output}"
        );
        assert!(
            !self.succeeded(),
            "{what}: the seeder must refuse, exit status {:?}\n{output}",
            self.status
        );
        assert!(
            output.contains("atom-seed: refused:"),
            "{what}: a guard must report itself as a refusal\n{output}"
        );
        output
    }
}

/// Runs the seeder with the given arguments and environment.
///
/// `None` removes the variable from the child, so a test can prove that a
/// variable being unset is accepted without depending on the parent process.
fn run_with(args: &[&str], database_url: Option<&str>, app_env: Option<&str>) -> SeedRun {
    let mut command = Command::new(SEEDER);
    command.args(args);

    match database_url {
        Some(url) => {
            command.env("DATABASE_URL", url);
        }
        None => {
            command.env_remove("DATABASE_URL");
        }
    }

    match app_env {
        Some(value) => {
            command.env("APP_ENV", value);
        }
        None => {
            command.env_remove("APP_ENV");
        }
    }

    let output = command
        .output()
        .expect("the atom-seed binary must be runnable");

    SeedRun {
        status: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Runs the seeder against the disposable test database as a development run.
fn run_in_test_database(args: &[&str]) -> SeedRun {
    let url = common::test_database_url();
    run_with(args, Some(&url), Some("dev"))
}

/// Reads one `entity inserted N skipped M` line out of the success report.
fn entity_count(output: &str, entity: &str) -> (i64, i64) {
    for line in output.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();

        if fields.len() == 5
            && fields[0] == entity
            && fields[1] == "inserted"
            && fields[3] == "skipped"
        {
            let inserted = fields[2]
                .parse::<i64>()
                .unwrap_or_else(|_| panic!("unparsable inserted count in {line:?}"));
            let skipped = fields[4]
                .parse::<i64>()
                .unwrap_or_else(|_| panic!("unparsable skipped count in {line:?}"));
            return (inserted, skipped);
        }
    }

    panic!("the report has no {entity:?} line; output was:\n{output}");
}

/// The id the seeder derives for `entity` row `index`.
///
/// This is the algorithm seeding.md §4.1 pins as part of the contract:
/// SHA-256 over `atom-seed/v1/<entity>/<index>`, the first sixteen bytes, with
/// the version and variant bits set. The tests recompute it rather than read it
/// back from the binary, so that an id the seeder stops deriving fails here
/// instead of silently changing which rows `--reset` owns.
fn seeded_id(entity: &str, index: usize) -> Uuid {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(format!("atom-seed/v1/{entity}/{index}").as_bytes());
    let digest = hasher.finalize();

    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    Uuid::from_bytes(bytes)
}

/// The ids of the two seeded accounts.
fn seeded_user_ids() -> Vec<Uuid> {
    vec![seeded_id("users", 0), seeded_id("users", 1)]
}

/// The ids of the seeded task rows.
fn seeded_task_ids(count: i64) -> Vec<Uuid> {
    (0..count)
        .map(|index| seeded_id("tasks", index as usize))
        .collect()
}

/// The ids of the seeded expense-claim rows.
fn seeded_claim_ids(count: i64) -> Vec<Uuid> {
    (0..count)
        .map(|index| seeded_id("expense_claims", index as usize))
        .collect()
}

/// The ids of the seeded expense-item rows.
///
/// The item index runs across the whole profile, not per claim (`C-R1`
/// §SEEDER), so one range covers every item.
fn seeded_item_ids(count: i64) -> Vec<Uuid> {
    (0..count)
        .map(|index| seeded_id("expense_items", index as usize))
        .collect()
}

/// The ids of the seeded company rows.
fn seeded_company_ids(count: i64) -> Vec<Uuid> {
    (0..count)
        .map(|index| seeded_id("companies", index as usize))
        .collect()
}

/// The ids of the seeded job-application rows.
fn seeded_application_ids(count: i64) -> Vec<Uuid> {
    (0..count)
        .map(|index| seeded_id("job_applications", index as usize))
        .collect()
}

/// A pool on the disposable test database.
async fn test_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::test_database_url())
        .await
        .expect("the test database must be reachable")
}

/// A pool whose database carries the embedded migrations.
///
/// A test that inserts a fixture row needs the schema whether or not the seeder
/// has already run in this process, so it applies the same migrations the
/// seeder and the rest of the suite apply.
async fn migrated_pool() -> PgPool {
    let pool = test_pool().await;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("the migrations must apply to the test database");

    pool
}

// ---------------------------------------------------------------------------
// A scratch database, for the one test the shared one cannot carry
//
// `A-C-S1-7` is a statement about the *order* of two steps, so proving it needs
// a database that has not been migrated yet. `atom_test` always has been, which
// is why the ordering was unprovable there: every comparison of
// `_sqlx_migrations` before and after a refused run compares a full table with
// itself.
// ---------------------------------------------------------------------------

/// The administration connection options of the suite.
fn admin_options() -> PgConnectOptions {
    PgConnectOptions::from_str(&common::test_database_url())
        .expect("the test database url must parse")
}

/// The URL of a database whose name this suite may create and drop.
///
/// `G2` admits an exact name or a `_dev` / `_test` / `_local` suffix, so the
/// suffix goes last; the `uuid` keeps two runs — or two copies of the suite —
/// from colliding. The whole name contains `test`, which is the rule the
/// harness applies to the suite and which this suite applies to what it creates.
fn scratch_database_url(name: &str) -> String {
    let base = common::test_database_url();
    let (prefix, _) = base
        .rsplit_once('/')
        .expect("the test database url names a database");
    format!("{prefix}/{name}")
}

/// A database this test created, dropped when it leaves scope.
///
/// Cleanup is a `Drop` implementation, not a call at the end of the test: an
/// assertion that fails aborts the test, and a scratch database left behind
/// would be inherited by the next run. `Drop` cannot await, so the removal owns
/// a single-threaded runtime on a thread of its own — the same arrangement
/// `tests/claims_schema.rs` uses, and it works while the test is unwinding.
struct ScratchDatabase {
    name: String,
    admin: PgConnectOptions,
    url: String,
}

impl ScratchDatabase {
    /// The URL of the scratch database itself.
    fn url(&self) -> &str {
        &self.url
    }

    /// Opens one connection to the scratch database.
    async fn connect(&self) -> PgConnection {
        PgConnection::connect(&self.url)
            .await
            .expect("the scratch database must be reachable")
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
///
/// `WITH (FORCE)` disconnects anything still attached (PostgreSQL 13 and later;
/// this project's floor is 14). The statement is built by `format('%I')` in the
/// server and the name is a **bound parameter**, so no identifier is ever
/// concatenated in Rust — a bind is impossible in `DROP DATABASE`, but a bound
/// name passed through the server's own quoting is the next best thing.
async fn drop_scratch_database(admin: &PgConnectOptions, name: &str) -> Result<(), sqlx::Error> {
    let mut connection = PgConnection::connect_with(admin).await?;

    let statement: String =
        sqlx::query_scalar("SELECT format('DROP DATABASE IF EXISTS %I WITH (FORCE)', $1)")
            .bind(name)
            .fetch_one(&mut connection)
            .await?;

    sqlx::query(&statement).execute(&mut connection).await?;

    Ok(())
}

/// Creates an empty database whose name is unique to this run.
async fn create_scratch_database() -> ScratchDatabase {
    let admin = admin_options();
    let name = format!("atom_g3_{}_test", Uuid::new_v4().simple());
    let url = scratch_database_url(&name);

    assert!(
        name.ends_with("_test") && name.contains("test"),
        "a database this suite creates must pass G2 and look disposable"
    );

    let mut connection = PgConnection::connect_with(&admin)
        .await
        .expect("the administration database must be reachable");

    // As above: the identifier is quoted by the server, from a bound value.
    let statement: String = sqlx::query_scalar("SELECT format('CREATE DATABASE %I', $1)")
        .bind(&name)
        .fetch_one(&mut connection)
        .await
        .expect("the scratch database name must be QUOTE_IDENT-able");

    sqlx::query(&statement)
        .execute(&mut connection)
        .await
        .expect("the atom role must be allowed to create a scratch database");

    ScratchDatabase { name, admin, url }
}

/// Creates the one table `G3` reads, and one account outside the seeded set.
///
/// The guard is `SELECT COUNT(*) FROM users WHERE id <> ALL($1)`, so `id` is the
/// only column it needs: the hand-made table is enough to make the preflight
/// refuse, which is all this test asks of it. The database stays unmigrated.
async fn plant_bare_users_table(scratch: &ScratchDatabase) -> Uuid {
    let mut connection = scratch.connect().await;

    sqlx::query("CREATE TABLE users (id uuid PRIMARY KEY)")
        .execute(&mut connection)
        .await
        .expect("the hand-made users table must be created");

    let foreign = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id) VALUES ($1)")
        .bind(foreign)
        .execute(&mut connection)
        .await
        .expect("the foreign account must be inserted");

    foreign
}

/// The base tables in `public` of the scratch database.
///
/// A migration is DDL, so this list is the second observation of the same fact
/// `_sqlx_migrations` records: before any migration it holds exactly the table
/// the test created, and after one it holds the schema.
async fn scratch_tables(scratch: &ScratchDatabase) -> Vec<String> {
    let mut connection = scratch.connect().await;

    sqlx::query_scalar(
        "SELECT table_name::text FROM information_schema.tables \
         WHERE table_schema = 'public' AND table_type = 'BASE TABLE' \
         ORDER BY table_name",
    )
    .fetch_all(&mut connection)
    .await
    .expect("the scratch database catalog must be readable")
}

/// Whether `_sqlx_migrations` exists in the scratch database.
async fn scratch_is_migrated(scratch: &ScratchDatabase) -> bool {
    let mut connection = scratch.connect().await;

    sqlx::query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
        .fetch_one(&mut connection)
        .await
        .expect("the scratch database must be probeable")
}

/// The accounts in the scratch database, by id.
async fn scratch_user_ids(scratch: &ScratchDatabase) -> Vec<Uuid> {
    let mut connection = scratch.connect().await;

    sqlx::query_scalar("SELECT id FROM users ORDER BY id")
        .fetch_all(&mut connection)
        .await
        .expect("the hand-made users table must be readable")
}

/// The row counts of the seeded dataset, restricted to the two seeded accounts.
async fn seeded_row_counts(pool: &PgPool) -> (i64, i64, i64, i64) {
    let ids = seeded_user_ids();

    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id = ANY($1)")
        .bind(ids.clone())
        .fetch_one(pool)
        .await
        .expect("the seeded users must be countable");

    let notes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notes WHERE user_id = ANY($1)")
        .bind(ids.clone())
        .fetch_one(pool)
        .await
        .expect("the seeded notes must be countable");

    let projects: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects WHERE user_id = ANY($1)")
        .bind(ids.clone())
        .fetch_one(pool)
        .await
        .expect("the seeded projects must be countable");

    // `tasks` has no `user_id`: ownership is transitive through the project.
    let tasks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tasks t JOIN projects p ON p.id = t.project_id WHERE p.user_id = ANY($1)",
    )
    .bind(ids)
    .fetch_one(pool)
    .await
    .expect("the seeded tasks must be countable");

    (users, notes, projects, tasks)
}

/// Inserts an account outside the seeded set and returns its id.
async fn insert_foreign_account(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    let email = common::unique_email("seeder-foreign");

    sqlx::query(
        "INSERT INTO users (id, email, password_hash, display_name) VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(&email)
    .bind("seeder-test-not-a-real-hash")
    .bind("Foreign Account")
    .execute(pool)
    .await
    .expect("the foreign account must be inserted");

    id
}

/// Whether an account with the given id exists.
async fn account_exists(pool: &PgPool, id: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM users WHERE id = $1)")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("the account must be readable")
}

/// The applied-migration state, or `None` when the migration table is absent.
///
/// `None` compares equal to `None`: a refusal on a database that was never
/// migrated must leave it never migrated.
async fn migration_state(pool: &PgPool) -> Option<(i64, i64)> {
    let present: bool = sqlx::query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
        .fetch_one(pool)
        .await
        .expect("the migration table must be probeable");

    if !present {
        return None;
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(pool)
        .await
        .expect("the migration table must be countable");

    let latest: Option<i64> = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(pool)
        .await
        .expect("the newest migration must be readable");

    Some((count, latest.unwrap_or(-1)))
}

/// The stored password hashes of the two seeded accounts, ordered by id.
async fn seeded_password_hashes(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT password_hash FROM users WHERE id = ANY($1) ORDER BY id",
    )
    .bind(seeded_user_ids())
    .fetch_all(pool)
    .await
    .expect("the seeded hashes must be readable")
}

/// A stored note: id, title, content, created_at, updated_at.
type NoteRow = (Uuid, String, String, DateTime<Utc>, DateTime<Utc>);

/// A stored task: id, status, created_at, updated_at, completed_at.
type TaskRow = (
    Uuid,
    String,
    DateTime<Utc>,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
);

/// The values two `--reset` runs must reproduce exactly.
#[derive(Debug, Default)]
struct Snapshot {
    users: Vec<(Uuid, DateTime<Utc>, DateTime<Utc>)>,
    notes: Vec<NoteRow>,
    projects: Vec<(Uuid, String, DateTime<Utc>, DateTime<Utc>)>,
    tasks: Vec<TaskRow>,
}

/// Reads the seeded dataset, ordered the way a client lists it.
async fn snapshot(pool: &PgPool) -> Snapshot {
    let ids = seeded_user_ids();

    let users = sqlx::query_as::<_, (Uuid, DateTime<Utc>, DateTime<Utc>)>(
        "SELECT id, created_at, updated_at FROM users WHERE id = ANY($1) ORDER BY id",
    )
    .bind(ids.clone())
    .fetch_all(pool)
    .await
    .expect("the seeded users must be readable");

    let notes = sqlx::query_as::<_, NoteRow>(
        "SELECT id, title, content, created_at, updated_at
         FROM notes WHERE user_id = ANY($1)
         ORDER BY created_at DESC, id",
    )
    .bind(ids.clone())
    .fetch_all(pool)
    .await
    .expect("the seeded notes must be readable");

    let projects = sqlx::query_as::<_, (Uuid, String, DateTime<Utc>, DateTime<Utc>)>(
        "SELECT id, name, created_at, updated_at
         FROM projects WHERE user_id = ANY($1)
         ORDER BY created_at DESC, id",
    )
    .bind(ids.clone())
    .fetch_all(pool)
    .await
    .expect("the seeded projects must be readable");

    let tasks = sqlx::query_as::<_, TaskRow>(
        "SELECT t.id, t.status::text, t.created_at, t.updated_at, t.completed_at
         FROM tasks t JOIN projects p ON p.id = t.project_id
         WHERE p.user_id = ANY($1)
         ORDER BY t.id",
    )
    .bind(ids)
    .fetch_all(pool)
    .await
    .expect("the seeded tasks must be readable");

    Snapshot {
        users,
        notes,
        projects,
        tasks,
    }
}

/// Asserts that two snapshots agree row by row.
///
/// The first difference is named by index, so a failure does not print two
/// hundred rows of note text.
fn assert_reproduced<T: PartialEq + std::fmt::Debug>(entity: &str, before: &[T], after: &[T]) {
    assert_eq!(
        before.len(),
        after.len(),
        "{entity}: two --reset runs must produce the same number of rows"
    );

    for (index, (before, after)) in before.iter().zip(after).enumerate() {
        assert_eq!(
            before, after,
            "{entity}: row {index} differs between two --reset runs"
        );
    }
}

#[tokio::test]
async fn the_seeder_refuses_a_production_environment() {
    // No DATABASE_URL at all: G1 runs before anything needs a database, so the
    // only way a run can name APP_ENV here is to refuse on it.
    for value in ["prod", "production", "staging", "Production", ""] {
        let run = run_with(&["--profile", "dev"], None, Some(value));
        let output = run.require_refusal(&format!("APP_ENV={value:?}"));

        assert!(
            output.contains("APP_ENV"),
            "the refusal must name the variable it refused; APP_ENV={value:?}\n{output}"
        );

        let normalised = value.trim().to_ascii_lowercase();
        assert!(
            output.to_lowercase().contains(&format!("\"{normalised}\"")),
            "the refusal must render the value it refused as a Rust string literal; APP_ENV={value:?}\n{output}"
        );

        for fragment in NEVER_PRINTED {
            assert!(
                !output.contains(fragment),
                "a G1 refusal must not print {fragment:?}; APP_ENV={value:?}\n{output}"
            );
        }
    }

    // The documented development values and an unset variable are allowed. The
    // run then fails because no DATABASE_URL was supplied, and any mention of
    // APP_ENV would mean G1 refused a value it must accept.
    for value in [None, Some("dev"), Some("development"), Some("local")] {
        let run = run_with(&["--profile", "dev"], None, value);
        let output = run.output();

        assert!(
            !run.succeeded(),
            "without DATABASE_URL the run must fail; APP_ENV={value:?} status {:?}\n{output}",
            run.status
        );
        assert!(
            !output.contains("APP_ENV"),
            "APP_ENV={value:?} is allowed, so the guard must not refuse it\n{output}"
        );
    }
}

#[tokio::test]
async fn the_seeder_refuses_a_database_whose_name_is_not_a_development_database() {
    // Both names contain "dev" or "test" as a substring and must still be
    // refused: the amended guard is a whole-name allow-list, not a substring
    // test. The port is closed, so a name refusal proves the guard did not
    // connect first.
    for name in ["atom", "contest"] {
        let url = format!("postgres://atom:atom@127.0.0.1:1/{name}");
        let run = run_with(&["--profile", "dev"], Some(&url), Some("dev"));
        let output = run.require_refusal(&format!("the database name {name:?}"));

        assert!(
            output.to_lowercase().contains("not allowed"),
            "the refusal must say the name is not allowed; name {name:?}\n{output}"
        );
        assert!(
            output.contains(&format!("\"{name}\"")),
            "the refusal must render the name as a Rust string literal; name {name:?}\n{output}"
        );

        // The rendered name is all that may survive from the URL.
        assert!(
            !output.contains("postgres://"),
            "a G2 refusal must not print the URL; name {name:?}\n{output}"
        );
        assert!(
            !output.contains("127.0.0.1"),
            "a G2 refusal must not print the host; name {name:?}\n{output}"
        );
    }

    // The documented names pass G2. The credentials are deliberately invalid,
    // so the run stops at authentication: the point is only that the name was
    // admitted, and no row is written to a database outside the suite.
    for name in ["atom_dev", "atom_demo"] {
        let url = format!("postgres://seeder_probe:seeder_probe@127.0.0.1:5432/{name}");
        let run = run_with(&["--profile", "dev"], Some(&url), Some("dev"));
        let output = run.output();

        assert!(
            !output.to_lowercase().contains("not allowed"),
            "{name} is on the allow-list and must not be refused by G2\n{output}"
        );
        assert!(
            !run.succeeded(),
            "the probe credentials cannot authenticate, so the run must fail; name {name:?} status {:?}\n{output}",
            run.status
        );
    }
}

#[tokio::test]
async fn the_seeder_refuses_a_database_that_holds_foreign_rows() {
    let _serialised = seeder_lock().await;
    let pool = migrated_pool().await;

    // The shared database already holds accounts from the rest of the suite.
    // This test adds its own so the precondition is stated here instead of
    // depending on what an earlier binary left behind.
    let foreign = insert_foreign_account(&pool).await;
    let migrations_before = migration_state(&pool).await;
    let counts_before = seeded_row_counts(&pool).await;

    let run = run_in_test_database(&["--profile", "minimal"]);
    let output = run.require_refusal("a database holding accounts outside the seeded set");

    assert!(
        output.contains("--allow-non-empty"),
        "a G3 refusal must name the flag that admits the database\n{output}"
    );
    assert!(
        !output.contains("postgres://"),
        "a G3 refusal must not print the URL\n{output}"
    );
    assert!(
        account_exists(&pool, foreign).await,
        "a refused run must not delete an account it does not own"
    );
    assert_eq!(
        migration_state(&pool).await,
        migrations_before,
        "a refused run must not apply a migration"
    );
    assert_eq!(
        seeded_row_counts(&pool).await,
        counts_before,
        "a refused run must not insert a seeded row"
    );
}

/// `A-C-S1-7`: the foreign-account guard runs **before** the migration step.
///
/// This is the test [`the_seeder_refuses_a_database_that_holds_foreign_rows`]
/// cannot be. That one starts from `atom_test`, which is already migrated, so
/// "a refused run did not apply a migration" compares a table with itself and
/// is true by construction. Here the database has never seen a migration: a
/// hand-made `users` table is all `G3` needs — the guard is one
/// `SELECT COUNT(*) FROM users WHERE id <> ALL($1)` — so the absence of
/// `_sqlx_migrations` *after* a refusal is a fact only a run that refused
/// before migrating can produce.
///
/// The control at the end removes the other explanation: if the migration step
/// were never reached in this database for any reason, the flag-alone run would
/// not create the table either.
#[tokio::test]
async fn the_foreign_account_guard_runs_before_the_migration_step() {
    // No `seeder_lock`: this test shares nothing with the rest of the suite. The
    // database is its own, the name is unique to this run, and no row of
    // `atom_test` is read or written.
    let scratch = create_scratch_database().await;
    let foreign = plant_bare_users_table(&scratch).await;

    // The premise. Without it the assertion after the refusal proves nothing,
    // which is exactly the defect this test exists to replace.
    assert!(
        !scratch_is_migrated(&scratch).await,
        "the scratch database must start without a migration table"
    );
    assert_eq!(
        scratch_tables(&scratch).await,
        vec!["users".to_string()],
        "the scratch database must hold the hand-made table and nothing else"
    );

    let run = run_with(&["--profile", "minimal"], Some(scratch.url()), Some("dev"));
    let output = run.require_refusal("a database holding an account outside the seeded set");

    // The refusal must be `G3` and not something else that happened to fail: it
    // names the guard's own sentence and the flag that admits the database.
    assert!(
        output.contains("outside the seeded set"),
        "the refusal must be the foreign-account guard\n{output}"
    );
    assert!(
        output.contains("--allow-non-empty"),
        "a G3 refusal must name the flag that admits the database\n{output}"
    );
    assert!(
        !output.contains("postgres://"),
        "a G3 refusal must not print the URL\n{output}"
    );

    // The claim. `_sqlx_migrations` is created by `sqlx::migrate!` before it
    // applies anything, so its presence here would mean the migration step ran
    // first and the preflight did not hold the ordering.
    assert!(
        !scratch_is_migrated(&scratch).await,
        "a G3 refusal must leave no migration table: the preflight runs before \
         sqlx::migrate! (A-C-S1-7). A migration table after the refusal means the \
         DDL came first.\n{output}"
    );

    // And no other DDL either: a migration would have added its tables.
    assert_eq!(
        scratch_tables(&scratch).await,
        vec!["users".to_string()],
        "a refused run must send the target no DDL at all\n{output}"
    );

    // Nothing deleted, nothing inserted: the hand-made row is the only account,
    // and the two seeded ids are not there.
    assert_eq!(
        scratch_user_ids(&scratch).await,
        vec![foreign],
        "a refused run must leave the database exactly as it was, row for row"
    );
    assert!(
        !seeded_user_ids().contains(&foreign),
        "the fixture account must not be one of the seeded ids"
    );

    // The control. `--allow-non-empty` satisfies the same guard on the same
    // database, and the run then reaches the migration step: the table appears.
    // Whether the migrations themselves succeed against a pre-existing `users`
    // table is not this test's business - only that the step was entered.
    let admitted = run_with(
        &["--profile", "minimal", "--allow-non-empty"],
        Some(scratch.url()),
        Some("dev"),
    );
    let admitted_output = admitted.output();
    assert!(
        !admitted_output.contains("atom-seed: refused:"),
        "the flag admits the database, so the run must get past every guard\n{admitted_output}"
    );
    assert!(
        scratch_is_migrated(&scratch).await,
        "once the guard is satisfied the migration step must run; if even this run \
         leaves no migration table, the absence above would not be about ordering\n{admitted_output}"
    );
}

#[tokio::test]
async fn an_override_flag_admits_a_database_that_holds_foreign_rows() {
    let _serialised = seeder_lock().await;
    let pool = migrated_pool().await;

    let foreign = insert_foreign_account(&pool).await;

    let run = run_in_test_database(&["--profile", "minimal", "--reset", "--allow-non-empty"]);
    run.require_success("--allow-non-empty");

    assert!(
        account_exists(&pool, foreign).await,
        "the flag admits the database; it must not let --reset delete a foreign account"
    );

    let seeded: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id = ANY($1)")
        .bind(seeded_user_ids())
        .fetch_one(&pool)
        .await
        .expect("the seeded accounts must be countable");
    assert_eq!(
        seeded, MINIMAL_USERS,
        "an admitted run must still insert both seeded accounts"
    );
}

#[tokio::test]
async fn the_dev_profile_inserts_the_documented_counts() {
    let _serialised = seeder_lock().await;
    let pool = migrated_pool().await;

    let run = run_in_test_database(&["--profile", "dev", "--reset", "--allow-non-empty"]);
    let output = run.require_success("the dev profile");

    // With --reset every row is inserted fresh, so the report must account for
    // the whole profile and skip nothing.
    assert_eq!(entity_count(&output, "users"), (DEV_USERS, 0));
    assert_eq!(entity_count(&output, "notes"), (DEV_NOTES, 0));
    assert_eq!(entity_count(&output, "projects"), (DEV_PROJECTS, 0));
    assert_eq!(entity_count(&output, "tasks"), (DEV_TASKS, 0));

    // The same counts, measured in the database and restricted to the two
    // seeded ids: `atom_test` holds other accounts, so a table-wide COUNT(*)
    // would be wrong.
    assert_eq!(
        seeded_row_counts(&pool).await,
        (DEV_USERS, DEV_NOTES, DEV_PROJECTS, DEV_TASKS),
        "the dev profile must insert the documented counts"
    );

    // The documented ownership split: every account gets rows, so an ownership
    // test always has a row on each side.
    for id in seeded_user_ids() {
        let notes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notes WHERE user_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("the notes of one account must be countable");
        assert!(notes > 0, "the seeded account {id} must own notes");

        let projects: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects WHERE user_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("the projects of one account must be countable");
        assert!(projects > 0, "the seeded account {id} must own projects");
    }
}

#[tokio::test]
async fn a_second_run_skips_every_existing_row() {
    let _serialised = seeder_lock().await;
    let pool = migrated_pool().await;

    // Start from a canonical dataset so the test does not depend on what an
    // earlier test left behind.
    run_in_test_database(&["--profile", "minimal", "--reset", "--allow-non-empty"])
        .require_success("the first run");

    let before = seeded_row_counts(&pool).await;
    assert_eq!(
        before,
        (
            MINIMAL_USERS,
            MINIMAL_NOTES,
            MINIMAL_PROJECTS,
            MINIMAL_TASKS
        )
    );

    let second = run_in_test_database(&["--profile", "minimal", "--allow-non-empty"]);
    let output = second.require_success("a second run without --reset");

    for (entity, expected) in [
        ("users", MINIMAL_USERS),
        ("notes", MINIMAL_NOTES),
        ("projects", MINIMAL_PROJECTS),
        ("tasks", MINIMAL_TASKS),
    ] {
        let (inserted, skipped) = entity_count(&output, entity);
        assert_eq!(
            inserted, 0,
            "a second run must insert no {entity}; output:\n{output}"
        );
        assert_eq!(
            skipped, expected,
            "a second run must report every {entity} row as skipped; output:\n{output}"
        );
    }

    assert_eq!(
        seeded_row_counts(&pool).await,
        before,
        "a second run must insert nothing"
    );
}

#[tokio::test]
async fn a_reset_run_reproduces_the_same_ids_and_timestamps() {
    let _serialised = seeder_lock().await;
    let pool = migrated_pool().await;

    let first = run_in_test_database(&["--profile", "dev", "--reset", "--allow-non-empty"]);
    let output = first.require_success("the first --reset run");
    assert!(
        output.contains("deleting every row owned by the two seeded accounts"),
        "a reset must print its blast radius before deleting (A-C-S1-9)\n{output}"
    );

    let before = snapshot(&pool).await;
    let hashes_before = seeded_password_hashes(&pool).await;

    let second = run_in_test_database(&["--profile", "dev", "--reset", "--allow-non-empty"]);
    second.require_success("the second --reset run");

    let after = snapshot(&pool).await;
    let hashes_after = seeded_password_hashes(&pool).await;

    assert_reproduced("users", &before.users, &after.users);
    assert_reproduced("notes", &before.notes, &after.notes);
    assert_reproduced("projects", &before.projects, &after.projects);
    assert_reproduced("tasks", &before.tasks, &after.tasks);

    assert_eq!(before.users.len(), 2, "both seeded accounts must exist");
    assert_eq!(
        hashes_before.len(),
        2,
        "both seeded hashes must be readable"
    );
    assert_eq!(hashes_after.len(), 2, "both seeded hashes must be readable");

    // The Argon2 salt is random by design (seeding.md §4.3). The hash is
    // therefore the one value a determinism claim must exclude: both rows of a
    // run share the hash the seeder computes once, and a fresh --reset run
    // recomputes it with a new salt.
    assert_eq!(
        hashes_before[0], hashes_before[1],
        "the two seeded accounts must share one hash"
    );
    assert_eq!(
        hashes_after[0], hashes_after[1],
        "the two seeded accounts must share one hash"
    );
    assert_ne!(
        hashes_before[0], hashes_after[0],
        "a --reset run must recompute the hash, so its random salt must differ between runs"
    );
}

#[tokio::test]
async fn the_seeder_never_prints_the_connection_string_or_a_hash() {
    let _serialised = seeder_lock().await;
    let pool = migrated_pool().await;

    let run = run_in_test_database(&["--profile", "dev", "--reset", "--allow-non-empty"]);
    let output = run.require_success("a successful dev run");

    assert!(
        entity_count(&output, "tasks").0 > 0,
        "the run must have printed a real report, otherwise the absence checks are vacuous\n{output}"
    );

    let url = common::test_database_url();
    for fragment in [url.as_str(), SEEDED_PASSWORD] {
        assert!(
            !output.contains(fragment),
            "a successful run must not print {fragment:?}\n{output}"
        );
    }
    for fragment in NEVER_PRINTED {
        assert!(
            !output.contains(fragment),
            "a successful run must not print {fragment:?}\n{output}"
        );
    }

    let hashes = seeded_password_hashes(&pool).await;
    for hash in &hashes {
        assert!(
            !output.contains(hash),
            "a successful run must not print the stored password hash\n{output}"
        );
    }
}

#[tokio::test]
async fn seeded_tasks_belong_to_projects_of_the_same_owner() {
    let _serialised = seeder_lock().await;
    let pool = migrated_pool().await;

    run_in_test_database(&["--profile", "dev", "--reset", "--allow-non-empty"])
        .require_success("the dev profile");

    let task_ids = seeded_task_ids(DEV_TASKS);
    let rows = sqlx::query_as::<_, (Uuid, Option<Uuid>)>(
        "SELECT t.id, p.user_id
         FROM tasks t LEFT JOIN projects p ON p.id = t.project_id
         WHERE t.id = ANY($1)",
    )
    .bind(task_ids.clone())
    .fetch_all(&pool)
    .await
    .expect("the seeded tasks must be readable");

    assert_eq!(
        rows.len(),
        task_ids.len(),
        "every task id the seeder derives must exist exactly once"
    );

    let owners = seeded_user_ids();
    for (task, owner) in rows {
        let owner = owner.unwrap_or_else(|| panic!("the seeded task {task} has no project"));
        assert!(
            owners.contains(&owner),
            "the seeded task {task} sits under a project owned by {owner}, which is not a seeded account"
        );
    }
}

#[tokio::test]
async fn every_seeded_task_has_a_consistent_completion_timestamp() {
    let _serialised = seeder_lock().await;
    let pool = migrated_pool().await;

    run_in_test_database(&["--profile", "dev", "--reset", "--allow-non-empty"])
        .require_success("the dev profile");

    let ids = seeded_task_ids(DEV_TASKS);
    let (done_without, other_with, done, open, in_progress) =
        sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(
            "SELECT COUNT(*) FILTER (WHERE status = 'done' AND completed_at IS NULL),
                COUNT(*) FILTER (WHERE status <> 'done' AND completed_at IS NOT NULL),
                COUNT(*) FILTER (WHERE status = 'done'),
                COUNT(*) FILTER (WHERE status = 'open'),
                COUNT(*) FILTER (WHERE status = 'in_progress')
         FROM tasks WHERE id = ANY($1)",
        )
        .bind(&ids)
        .fetch_one(&pool)
        .await
        .expect("the seeded tasks must be readable");

    assert_eq!(done_without, 0, "a done task must carry completed_at");
    assert_eq!(other_with, 0, "only a done task may carry completed_at");

    // The consistency rule must not pass because the profile happens to contain
    // no done task, and the dev profile must exercise every status.
    assert!(
        done > 0,
        "the check must not be vacuous: done tasks must exist"
    );
    assert!(open > 0, "the dev profile must contain open tasks");
    assert!(
        in_progress > 0,
        "the dev profile must contain in-progress tasks"
    );
    assert_eq!(
        done + open + in_progress,
        DEV_TASKS,
        "every seeded task must carry one of the three statuses"
    );
}

#[tokio::test]
async fn a_connect_failure_prints_no_part_of_the_url() {
    let run = run_with(&["--profile", "dev"], Some(UNREACHABLE_URL), Some("dev"));
    let output = run.output();

    assert!(
        !run.succeeded(),
        "nothing listens on port 1, so the run must fail, status {:?}\n{output}",
        run.status
    );
    assert!(
        !output.trim().is_empty(),
        "the failure must be reported, not silent"
    );

    for fragment in ["postgres://", "atom:atom", "127.0.0.1"] {
        assert!(
            !output.contains(fragment),
            "a connect failure must not print {fragment:?}; the error classification must not carry the URL\n{output}"
        );
    }
}

#[tokio::test]
async fn a_constraint_violation_is_reported_by_sqlstate_and_constraint_only() {
    let _serialised = seeder_lock().await;
    let pool = migrated_pool().await;

    // A foreign account holding the documented owner address. The seeder's
    // insert conflicts on `users_email_unique_idx`, a real database constraint,
    // which is the only deterministic way to make it fail after the guards.
    sqlx::query("DELETE FROM users WHERE email = $1")
        .bind(SEEDED_EMAILS[0])
        .execute(&pool)
        .await
        .expect("the fixture must be able to free the documented address");

    let foreign = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, display_name) VALUES ($1, $2, $3, $4)",
    )
    .bind(foreign)
    .bind(SEEDED_EMAILS[0])
    .bind("seeder-test-not-a-real-hash")
    .bind("Foreign Owner")
    .execute(&pool)
    .await
    .expect("the foreign account must be inserted");

    let run = run_in_test_database(&["--profile", "minimal", "--reset", "--allow-non-empty"]);
    let output = run.output();

    // Clean up before asserting, so a failing assertion cannot poison every
    // later run with a leftover row at the documented address.
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(foreign)
        .execute(&pool)
        .await
        .expect("the fixture account must be removable");

    assert!(
        !run.succeeded(),
        "a unique violation must fail the run, status {:?}\n{output}",
        run.status
    );
    assert!(
        output.contains("23505"),
        "the SQLSTATE of a unique violation must be reported\n{output}"
    );
    assert!(
        output.contains("UniqueViolation"),
        "the violation kind must be reported (A-C-S1-8: kind() and code())\n{output}"
    );
    assert!(
        !output.contains("DETAIL:"),
        "the driver detail quotes the conflicting row and must not be printed\n{output}"
    );
    assert!(
        !output.contains(SEEDED_PASSWORD),
        "a failed run must not print the password\n{output}"
    );
    for fragment in NEVER_PRINTED {
        assert!(
            !output.contains(fragment),
            "a failed run must not print {fragment:?}\n{output}"
        );
    }
}

#[tokio::test]
async fn the_help_text_states_the_blast_radius_without_a_url_or_a_hash() {
    // No DATABASE_URL and no APP_ENV: --help must not need either.
    let run = run_with(&["--help"], None, None);
    let output = run.require_success("--help");

    for flag in ["--profile", "--reset", "--allow-non-empty", "--help"] {
        assert!(
            output.contains(flag),
            "--help must document {flag}\n{output}"
        );
    }

    // A-C-S1-9: the blast radius is stated before the flag is used.
    assert!(
        output
            .to_lowercase()
            .contains("deletes every row owned by the two seeded accounts"),
        "--help must state what --reset deletes\n{output}"
    );

    for fragment in NEVER_PRINTED {
        assert!(
            !output.contains(fragment),
            "--help must not print {fragment:?}\n{output}"
        );
    }
}

#[tokio::test]
async fn an_unknown_flag_is_a_usage_error_that_names_the_flag() {
    let run = run_with(&["--frobnicate"], None, None);
    let output = run.output();

    assert!(
        !run.succeeded(),
        "an unknown flag must exit non-zero, status {:?}\n{output}",
        run.status
    );
    assert!(
        output.contains("--frobnicate"),
        "a usage error must name the flag it rejected\n{output}"
    );
    assert!(
        !output.contains("atom-seed: refused:"),
        "a usage error is not a guard refusal\n{output}"
    );
}

#[tokio::test]
async fn the_seeded_accounts_are_the_published_development_credentials() {
    let _serialised = seeder_lock().await;
    let pool = migrated_pool().await;

    run_in_test_database(&["--profile", "minimal", "--reset", "--allow-non-empty"])
        .require_success("the minimal profile");

    let rows = sqlx::query_as::<_, (String, String, bool)>(
        "SELECT email, display_name, is_active FROM users WHERE id = ANY($1)",
    )
    .bind(seeded_user_ids())
    .fetch_all(&pool)
    .await
    .expect("the seeded accounts must be readable");

    assert_eq!(
        rows.len(),
        2,
        "the seeder must insert exactly the two documented accounts"
    );

    let mut published = SEEDED_EMAILS.to_vec();
    published.sort_unstable();
    let mut stored: Vec<String> = rows.iter().map(|(email, _, _)| email.clone()).collect();
    stored.sort();
    assert_eq!(stored, published);

    for (email, display_name, is_active) in &rows {
        let expected = match email.as_str() {
            "owner@atom.local" => "Owner",
            "other@atom.local" => "Other",
            other => panic!("unexpected seeded account {other:?}"),
        };
        assert_eq!(display_name, expected, "unexpected display name");
        assert!(is_active, "the seeded account {email} must be active");
    }

    // The published password must verify against the stored PHC string, with
    // the parameters carried by that string.
    for hash in seeded_password_hashes(&pool).await {
        let parsed = PasswordHash::new(&hash)
            .unwrap_or_else(|error| panic!("the stored hash must be a PHC string: {error}"));
        Argon2::default()
            .verify_password(SEEDED_PASSWORD.as_bytes(), &parsed)
            .expect("the published password must verify against the stored hash");
    }
}

#[tokio::test]
async fn a_reset_deletes_rows_owned_by_the_seeded_accounts() {
    let _serialised = seeder_lock().await;
    let pool = migrated_pool().await;

    run_in_test_database(&["--profile", "minimal", "--reset", "--allow-non-empty"])
        .require_success("the first run");

    // A row a developer could have written through the API while logged in as
    // the seeded owner. A-C-S1-9: the reset predicate is the owner, not "a row
    // the seeder inserted", so this row must be deleted.
    let note = Uuid::new_v4();
    sqlx::query("INSERT INTO notes (id, user_id, title, content) VALUES ($1, $2, $3, $4)")
        .bind(note)
        .bind(seeded_id("users", 0))
        .bind("a note written through the api")
        .bind("body of a note written through the api")
        .execute(&pool)
        .await
        .expect("the developer note must be inserted");

    run_in_test_database(&["--profile", "minimal", "--reset", "--allow-non-empty"])
        .require_success("the reset run");

    let present: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM notes WHERE id = $1)")
        .bind(note)
        .fetch_one(&pool)
        .await
        .expect("the developer note must be probeable");

    assert!(
        !present,
        "--reset must delete every row owned by the two seeded accounts (A-C-S1-9)"
    );
    assert_eq!(
        seeded_row_counts(&pool).await,
        (
            MINIMAL_USERS,
            MINIMAL_NOTES,
            MINIMAL_PROJECTS,
            MINIMAL_TASKS
        ),
        "the documented dataset must be intact after the reset"
    );
}

#[tokio::test]
async fn the_seeder_dev_profile_inserts_the_documented_claim_counts() {
    let _serialised = seeder_lock().await;
    let pool = migrated_pool().await;

    run_in_test_database(&["--profile", "dev", "--reset", "--allow-non-empty"])
        .require_success("the dev profile");

    // `atom_test` is shared, so every count is restricted to the two seeded
    // accounts. A table-wide COUNT(*) would count another test's rows.
    let accounts = seeded_user_ids();

    let claims: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM expense_claims WHERE user_id = ANY($1)")
            .bind(accounts.clone())
            .fetch_one(&pool)
            .await
            .expect("the seeded claims must be countable");
    assert_eq!(
        claims, DEV_CLAIMS,
        "the dev profile must insert the documented claim count"
    );

    let items: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM expense_items WHERE user_id = ANY($1)")
            .bind(accounts.clone())
            .fetch_one(&pool)
            .await
            .expect("the seeded items must be countable");
    assert_eq!(
        items, DEV_CLAIM_ITEMS,
        "the dev profile must insert the documented item count"
    );

    // The counts alone would also pass if the rows sat on different
    // deterministic slots, so the ids the seeder derives are checked too. This
    // is the derivation `--reset` and the total refresh both rely on.
    let claim_ids = seeded_claim_ids(DEV_CLAIMS);
    let claims_with_ids: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM expense_claims WHERE id = ANY($1)")
            .bind(&claim_ids)
            .fetch_one(&pool)
            .await
            .expect("the derived claim ids must be countable");
    assert_eq!(
        claims_with_ids, DEV_CLAIMS,
        "every claim id must be SHA-256(\"atom-seed/v1/expense_claims/<index>\")"
    );

    let item_ids = seeded_item_ids(DEV_CLAIM_ITEMS);
    let items_with_ids: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM expense_items WHERE id = ANY($1)")
            .bind(&item_ids)
            .fetch_one(&pool)
            .await
            .expect("the derived item ids must be countable");
    assert_eq!(
        items_with_ids, DEV_CLAIM_ITEMS,
        "every item id must be SHA-256(\"atom-seed/v1/expense_items/<index>\")"
    );

    // The first claim is the documented draft with no items (seeding.md §7.4).
    let first = seeded_id("expense_claims", 0);
    let (status, owner): (String, Uuid) =
        sqlx::query_as("SELECT status::text, user_id FROM expense_claims WHERE id = $1")
            .bind(first)
            .fetch_one(&pool)
            .await
            .expect("the first seeded claim must be readable");
    assert_eq!(status, "draft", "claim 0 must be the profile's draft claim");
    assert!(
        accounts.contains(&owner),
        "the first seeded claim must belong to a seeded account"
    );

    let first_items: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM expense_items WHERE claim_id = $1")
            .bind(first)
            .fetch_one(&pool)
            .await
            .expect("the items of the first claim must be countable");
    assert_eq!(
        first_items, 0,
        "the first draft claim must carry no items (seeding.md §7.4)"
    );

    // And it is the only claim without items, so the assertion above is a
    // property of that claim rather than of the whole profile.
    let itemless: Vec<Uuid> = sqlx::query_scalar(
        "SELECT c.id FROM expense_claims c
         WHERE c.id = ANY($1)
           AND NOT EXISTS (SELECT 1 FROM expense_items i WHERE i.claim_id = c.id)
         ORDER BY c.id",
    )
    .bind(&claim_ids)
    .fetch_all(&pool)
    .await
    .expect("the itemless claims must be listable");
    assert_eq!(
        itemless,
        vec![first],
        "only the first draft claim may have no items"
    );
}

#[tokio::test]
async fn a_seeded_claim_total_equals_the_sum_of_its_items() {
    let _serialised = seeder_lock().await;
    let pool = migrated_pool().await;

    run_in_test_database(&["--profile", "dev", "--reset", "--allow-non-empty"])
        .require_success("the dev profile");

    let claim_ids = seeded_claim_ids(DEV_CLAIMS);

    // The comparison is done by PostgreSQL in `numeric`, never in Rust: the
    // crate has no decimal type (ADR-0007), and a text comparison would fail on
    // a trailing zero rather than on a wrong amount.
    let mismatched: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT c.id, c.total_amount::text, COALESCE(SUM(i.amount), 0)::text
         FROM expense_claims c
         LEFT JOIN expense_items i ON i.claim_id = c.id
         WHERE c.id = ANY($1)
         GROUP BY c.id, c.total_amount
         HAVING c.total_amount <> COALESCE(SUM(i.amount), 0)
         ORDER BY c.id",
    )
    .bind(&claim_ids)
    .fetch_all(&pool)
    .await
    .expect("the seeded claims must be readable");

    assert!(
        mismatched.is_empty(),
        "every seeded claim's total_amount must equal SUM(amount) of its items; \
         mismatches (claim, stored total, sum of items): {mismatched:?}"
    );

    // The equality above is only meaningful if the claims are there: a missing
    // dataset would produce no mismatches.
    let present: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM expense_claims WHERE id = ANY($1)")
        .bind(&claim_ids)
        .fetch_one(&pool)
        .await
        .expect("the seeded claims must be countable");
    assert_eq!(
        present, DEV_CLAIMS,
        "every seeded claim must be present for the sum check to mean anything"
    );

    // Every seeded claim is in the table, so the lifecycle constraint accepted
    // it. This asserts that the constraint relied on is the one this schema
    // carries, so the coverage cannot be vacuous.
    let enforced: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM pg_constraint
             WHERE conname = 'expense_claims_lifecycle_timestamps_chk'
               AND conrelid = 'expense_claims'::regclass
               AND contype = 'c'
         )",
    )
    .fetch_one(&pool)
    .await
    .expect("the constraint catalogue must be readable");
    assert!(
        enforced,
        "the lifecycle constraint must be the one this table carries"
    );

    // All five statuses are represented, so every arm of that constraint has
    // data behind it rather than being satisfied by an absence.
    let statuses: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT status::text FROM expense_claims WHERE id = ANY($1) ORDER BY 1",
    )
    .bind(&claim_ids)
    .fetch_all(&pool)
    .await
    .expect("the seeded claim statuses must be readable");
    assert_eq!(
        statuses,
        vec!["approved", "draft", "paid", "rejected", "submitted"],
        "the dev profile must exercise every status the lifecycle constraint constrains"
    );
}

#[tokio::test]
async fn the_seeder_dev_profile_inserts_the_documented_company_and_application_counts() {
    let _serialised = seeder_lock().await;
    let pool = migrated_pool().await;

    let run = run_in_test_database(&["--profile", "dev", "--reset", "--allow-non-empty"]);
    let output = run.require_success("the dev profile");

    // With --reset every row is inserted fresh, so the report must account for
    // the whole profile and skip nothing.
    // The report names the entity the way the rest of the seeder does -
    // `applications`, not `job_applications` - while the derived id uses the
    // table name. Both forms are checked, each where it is used.
    assert_eq!(entity_count(&output, "companies"), (DEV_COMPANIES, 0));
    assert_eq!(entity_count(&output, "applications"), (DEV_APPLICATIONS, 0));
    assert_eq!(entity_count(&output, "events"), (DEV_EVENTS, 0));

    // `atom_test` is shared, so every count is restricted to the two seeded
    // accounts. A table-wide COUNT(*) would count another test's rows.
    let accounts = seeded_user_ids();

    let companies: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM companies WHERE user_id = ANY($1)")
            .bind(accounts.clone())
            .fetch_one(&pool)
            .await
            .expect("the seeded companies must be countable");
    assert_eq!(
        companies, DEV_COMPANIES,
        "the database must agree with the count the seeder reported"
    );

    let applications: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM job_applications WHERE user_id = ANY($1)")
            .bind(accounts.clone())
            .fetch_one(&pool)
            .await
            .expect("the seeded applications must be countable");
    assert_eq!(
        applications, DEV_APPLICATIONS,
        "the database must agree with the count the seeder reported"
    );

    let events: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM application_events WHERE user_id = ANY($1)")
            .bind(accounts.clone())
            .fetch_one(&pool)
            .await
            .expect("the seeded events must be countable");
    assert_eq!(
        events, DEV_EVENTS,
        "the database must agree with the count the seeder reported"
    );

    // The counts alone would also pass if the rows sat on different
    // deterministic slots, so the ids the seeder derives are checked too. This
    // is the derivation `--reset` relies on: a re-inserted parent gets the same
    // id, so a stale child would re-attach instead of being noticed.
    let company_ids = seeded_company_ids(DEV_COMPANIES);
    let companies_with_ids: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM companies WHERE id = ANY($1)")
            .bind(&company_ids)
            .fetch_one(&pool)
            .await
            .expect("the derived company ids must be countable");
    assert_eq!(
        companies_with_ids, DEV_COMPANIES,
        "every company id must be SHA-256(\"atom-seed/v1/companies/<index>\")"
    );

    let application_ids = seeded_application_ids(DEV_APPLICATIONS);
    let applications_with_ids: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM job_applications WHERE id = ANY($1)")
            .bind(&application_ids)
            .fetch_one(&pool)
            .await
            .expect("the derived application ids must be countable");
    assert_eq!(
        applications_with_ids, DEV_APPLICATIONS,
        "every application id must be SHA-256(\"atom-seed/v1/job_applications/<index>\")"
    );

    // Every application's company must have the application's owner: this is
    // the invariant the composite key enforces, so a seeded row that broke it
    // could not have been inserted at all. The check proves the data exercises
    // the rule rather than the rule merely existing.
    let mismatched: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM job_applications a
         JOIN companies c ON c.id = a.company_id
         WHERE a.id = ANY($1) AND c.user_id <> a.user_id",
    )
    .bind(&application_ids)
    .fetch_one(&pool)
    .await
    .expect("the seeded applications must be readable with their companies");
    assert_eq!(
        mismatched, 0,
        "every seeded application's company must have the same owner"
    );

    // The join above is only meaningful if every application actually resolves
    // a company, so the resolution is asserted rather than assumed.
    let unresolved: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM job_applications a
         LEFT JOIN companies c ON c.id = a.company_id
         WHERE a.id = ANY($1) AND c.id IS NULL",
    )
    .bind(&application_ids)
    .fetch_one(&pool)
    .await
    .expect("the seeded applications must be readable with their companies");
    assert_eq!(
        unresolved, 0,
        "every seeded application must resolve a company for the owner check to mean anything"
    );

    // A currency is required whenever a salary bound is present
    // (`job_applications_salary_currency_required_chk`, `JOB-R03`). A seeded
    // row that broke it could not be stored, so this asserts the data the
    // profile ships rather than the constraint.
    let missing_currency: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM job_applications
         WHERE id = ANY($1)
           AND (salary_min IS NOT NULL OR salary_max IS NOT NULL)
           AND salary_currency IS NULL",
    )
    .bind(&application_ids)
    .fetch_one(&pool)
    .await
    .expect("the seeded applications must be countable by salary");
    assert_eq!(
        missing_currency, 0,
        "a seeded salary bound must always carry a currency"
    );

    // And a currency must not appear without a bound.
    let bare_currency: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM job_applications
         WHERE id = ANY($1)
           AND salary_currency IS NOT NULL
           AND salary_min IS NULL
           AND salary_max IS NULL",
    )
    .bind(&application_ids)
    .fetch_one(&pool)
    .await
    .expect("the seeded applications must be countable by currency");
    assert_eq!(
        bare_currency, 0,
        "a seeded application must not carry a currency with no salary bound"
    );

    // The two rules above must not pass only because the profile carries no
    // salary at all. The seeder gives a bound to two applications in every
    // three, so the data behind the rule is asserted rather than assumed, and
    // the currency count must equal it: one currency per bounded application,
    // none without a bound.
    let (with_bound, with_currency): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE salary_min IS NOT NULL OR salary_max IS NOT NULL),
                COUNT(*) FILTER (WHERE salary_currency IS NOT NULL)
         FROM job_applications
         WHERE id = ANY($1)",
    )
    .bind(&application_ids)
    .fetch_one(&pool)
    .await
    .expect("the seeded applications must be countable by salary");
    assert!(
        with_bound > 0,
        "the dev profile must carry salary bounds, otherwise the currency rules have no data"
    );
    assert_eq!(
        with_currency, with_bound,
        "every seeded application with a salary bound must carry exactly one currency"
    );
}

// ---------------------------------------------------------------------------
// S4 - the embeddings, behind `--with-embeddings`
// ---------------------------------------------------------------------------

/// The embedder the seeder constructs directly (`ADR-0008` §7.2).
///
/// The seeder does not read `ml.backend` and does not read a model file, so its
/// rows carry this identifier whatever the deployment's ML settings are.
#[cfg(feature = "ml")]
const SEEDED_MODEL_ID: &str = "deterministic-sha256-v1-384";

/// The vector length that embedder produces.
#[cfg(feature = "ml")]
const SEEDED_DIMENSION: usize = 384;

/// The tolerance of a unit-norm comparison on a seeded vector.
///
/// `embeddings.vector` is `REAL[]`, so every component is rounded to `f32`.
/// 384 of those roundings make an exact `1.0` unreachable, and `1e-5` is the
/// bound `ADR-0008` §9 states for the stored representation.
#[cfg(feature = "ml")]
const SEEDED_UNIT_NORM_TOLERANCE: f64 = 1e-5;

/// The number of embedding rows owned by the two seeded accounts.
#[cfg(feature = "ml")]
async fn seeded_embedding_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM embeddings WHERE user_id = ANY($1)")
        .bind(seeded_user_ids())
        .fetch_one(pool)
        .await
        .expect("the seeded embeddings must be countable")
}

/// One seeded embedding row: source type, model id, dim, vector.
#[cfg(feature = "ml")]
type SeededEmbedding = (String, String, i32, Vec<f32>);

/// Every embedding row of the two seeded accounts.
#[cfg(feature = "ml")]
async fn seeded_embeddings(pool: &PgPool) -> Vec<SeededEmbedding> {
    sqlx::query_as::<_, SeededEmbedding>(
        "SELECT source_type, model_id, dim, vector
         FROM embeddings
         WHERE user_id = ANY($1)
         ORDER BY source_type, source_id",
    )
    .bind(seeded_user_ids())
    .fetch_all(pool)
    .await
    .expect("the seeded embeddings must be readable")
}

/// The Euclidean norm, in `f64` so the sum does not lose precision.
#[cfg(feature = "ml")]
fn seeded_l2(values: &[f32]) -> f64 {
    values
        .iter()
        .map(|value| f64::from(*value) * f64::from(*value))
        .sum::<f64>()
        .sqrt()
}

/// `S4`: the seeder writes embeddings **only** when it is asked to.
///
/// Two halves, because either alone is a weaker claim. Without the flag the
/// table stays empty for the seeded accounts - a seeder that embedded by
/// default would make every `--reset` cost 220 inferences nobody asked for.
/// With it, one row exists per note and per application, every vector is unit
/// length, and the row names the deterministic embedder that `ADR-0008` §7.2
/// makes the seeder construct directly.
#[cfg(feature = "ml")]
#[tokio::test]
async fn the_seeder_writes_embeddings_only_with_the_flag() {
    let _serialised = seeder_lock().await;
    let pool = migrated_pool().await;

    // --- without the flag ---------------------------------------------------
    run_in_test_database(&["--profile", "dev", "--reset", "--allow-non-empty"])
        .require_success("the dev profile without --with-embeddings");

    assert_eq!(
        seeded_embedding_count(&pool).await,
        0,
        "a run without --with-embeddings must write no embedding row"
    );

    // --- with the flag ------------------------------------------------------
    let run = run_in_test_database(&[
        "--profile",
        "dev",
        "--reset",
        "--allow-non-empty",
        "--with-embeddings",
    ]);
    let output = run.require_success("the dev profile with --with-embeddings");

    // One row per note and per application (`architecture.md` §12.7). Nothing
    // else is a source: `source_type` has exactly those two values.
    let sources = DEV_NOTES + DEV_APPLICATIONS;
    assert_eq!(
        entity_count(&output, "embeddings"),
        (sources, 0),
        "--reset inserts every embedding fresh, so the report must account for all {sources}\n{output}"
    );

    assert_eq!(
        seeded_embedding_count(&pool).await,
        sources,
        "the database must agree with the count the seeder reported"
    );

    let (notes, applications): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE source_type = 'note'),
                COUNT(*) FILTER (WHERE source_type = 'application')
         FROM embeddings
         WHERE user_id = ANY($1)",
    )
    .bind(seeded_user_ids())
    .fetch_one(&pool)
    .await
    .expect("the seeded embeddings must be countable by source type");
    assert_eq!(notes, DEV_NOTES, "one row per seeded note");
    assert_eq!(
        applications, DEV_APPLICATIONS,
        "one row per seeded application"
    );

    // Every source really exists: a row whose source does not would be an
    // orphan the reindex sweep has to clean, and the seeder must not create one.
    let orphans: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM embeddings e
         WHERE e.user_id = ANY($1)
           AND NOT EXISTS (
                 SELECT 1 FROM notes n
                  WHERE e.source_type = 'note'
                    AND n.id = e.source_id
                    AND n.user_id = e.user_id
               )
           AND NOT EXISTS (
                 SELECT 1 FROM job_applications a
                  WHERE e.source_type = 'application'
                    AND a.id = e.source_id
                    AND a.user_id = e.user_id
               )",
    )
    .bind(seeded_user_ids())
    .fetch_one(&pool)
    .await
    .expect("the seeded embeddings must be readable with their sources");
    assert_eq!(orphans, 0, "every seeded embedding must name a real source");

    let rows = seeded_embeddings(&pool).await;
    assert_eq!(
        rows.len(),
        usize::try_from(sources).expect("the count fits"),
    );

    for (source_type, model_id, dim, vector) in &rows {
        assert_eq!(
            model_id, SEEDED_MODEL_ID,
            "the seeder constructs the deterministic embedder directly, so its rows name it \
             ({source_type})"
        );
        assert_eq!(
            *dim,
            i32::try_from(SEEDED_DIMENSION).expect("the dimension fits"),
            "the row records the vector length ({source_type})"
        );
        assert_eq!(
            vector.len(),
            SEEDED_DIMENSION,
            "the vector has the declared number of elements ({source_type})"
        );

        let norm = seeded_l2(vector);
        assert!(
            (1.0 - norm).abs() <= SEEDED_UNIT_NORM_TOLERANCE,
            "a seeded vector is unit length like every stored vector: {source_type} has norm \
             {norm}, outside 1.0 +/- {SEEDED_UNIT_NORM_TOLERANCE}"
        );
    }

    // A second run with the flag and without --reset must skip, not rewrite:
    // the rows are deterministic, so there is nothing to change.
    let second =
        run_in_test_database(&["--profile", "dev", "--allow-non-empty", "--with-embeddings"]);
    let output = second.require_success("a second run with --with-embeddings");
    assert_eq!(
        entity_count(&output, "embeddings"),
        (0, sources),
        "a second run must report every embedding as skipped\n{output}"
    );
    assert_eq!(
        seeded_embedding_count(&pool).await,
        sources,
        "a second run must insert no embedding row"
    );
}
