//! `atom-seed` - deterministic development data for the ATOM database.
//!
//! The binary fills a disposable PostgreSQL database with the Phase 1
//! entities - `users`, `notes`, `projects` and `tasks` - and with the Phase 2
//! `expense_claims` and `expense_items` rows that the reimbursements feature
//! reads. It is specified in `docs/database/seeding.md`, locked by the `C-S1`
//! contract (with the security amendments `A-C-S1-5` ... `A-C-S1-10`) and
//! extended by `C-R1` §SEEDER, both in
//! `docs/engineering/runs/chain/ledger.md`.
//!
//! ```text
//! cargo run --bin atom-seed -- --profile minimal --reset
//! ```
//!
//! Safety properties:
//!
//! 1. `APP_ENV` is an allow-list (`development`, `dev`, `local`, or unset).
//! 2. The database name is an allow-list (`atom_dev`, `atom_test`,
//!    `atom_local`, `atom_demo`, or a `_dev` / `_test` / `_local` suffix), read
//!    through the driver's own parser and confirmed against
//!    `current_database()`.
//! 3. The foreign-account guard runs before the migration step and again
//!    inside the transaction.
//! 4. Nothing derived from `DATABASE_URL` or a row is ever printed. Every
//!    `sqlx::Error` and `sqlx::migrate::MigrateError` variant is classified
//!    into a fixed message, a driver kind and a SQLSTATE at most.
//!
//! Determinism: every id is `SHA-256("atom-seed/v1/<entity>/<index>")` and
//! every timestamp derives from the fixed base `2026-01-01T00:00:00Z`. The one
//! deliberate exception is the Argon2 salt: the hash is computed once per run
//! from a random salt and reused for both accounts, because a fixed salt would
//! be worse for a credential that is already public.
//!
//! The binary deliberately does not link the `atom_api` library: it needs
//! nothing from it, and importing the crate would pull its whole build graph -
//! including the vendored Swagger UI bundle - into a development tool.

#![deny(clippy::unwrap_used, clippy::expect_used)]

use std::process::ExitCode;
use std::str::FromStr;

use argon2::password_hash::SaltString;
use argon2::{Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version};
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use uuid::Uuid;

/// The binding credential for both seeded accounts.
///
/// It is published in `docs/database/seeding.md`, it is fake, and it must
/// never be printed by this binary.
const SEEDED_PASSWORD: &str = "atom-dev-password-1234";

/// The two seeded accounts: `(email, display_name)`.
const SEEDED_USERS: [(&str, &str); 2] =
    [("owner@atom.local", "Owner"), ("other@atom.local", "Other")];

/// The Argon2id parameters the stored hash is created with.
///
/// They mirror `config/auth.toml`. They are constants rather than a read of
/// the configuration directory because the seeder must run without
/// `APP_JWT_SECRET` and because a PHC string carries its own parameters.
const ARGON2_MEMORY_KIB: u32 = 19_456;
const ARGON2_ITERATIONS: u32 = 2;
const ARGON2_PARALLELISM: u32 = 1;

/// Bound on the initial connection attempt, so an unreachable host cannot
/// hang the process for the operating system's connect timeout.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Every table the seeder handles: the six it inserts into and
/// `refresh_sessions`, which `--reset` deletes but the seeder never inserts.
///
/// `V6` compares this list with the schema and fails the run when a table
/// exists that the list does not name. That is what makes "add a table, extend
/// the seeder" enforceable.
const SEEDED_TABLES: [&str; 7] = [
    "expense_claims",
    "expense_items",
    "notes",
    "projects",
    "refresh_sessions",
    "tasks",
    "users",
];

/// The one-line reset warning, printed before the first delete (A-C-S1-9).
const RESET_WARNING: &str = "atom-seed: reset: deleting every row owned by the two seeded accounts (owner@atom.local and other@atom.local), including rows created through the API while logged in as them";

/// The usage text. It names the blast radius before the flag is used and
/// contains no secret.
const USAGE: &str = "\
atom-seed - deterministic development data for the ATOM database.

Usage:
  cargo run --bin atom-seed -- [--profile <minimal|dev|demo>] [--reset] [--allow-non-empty]
  cargo run --bin atom-seed -- --help

Options:
  --profile <name>   Data volume: minimal, dev (default) or demo.
  --reset            Delete every row owned by the two seeded accounts, then insert.
                     Deletes every row owned by the two seeded accounts, including rows you created through the API while logged in as them.
  --allow-non-empty  Admit a database that holds accounts outside the seeded set.
  --help             Print this text and exit with status 0.

Environment:
  DATABASE_URL       Required. It must name a development database; its value is never printed.
  APP_ENV            Optional. Unset, \"development\", \"dev\" and \"local\" are allowed.

The seeder is development-only: it writes published development credentials.
The two seeded accounts are owner@atom.local and other@atom.local.
";

/// The dataset size selected by `--profile`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Profile {
    /// The smallest dataset that still covers every entity.
    Minimal,
    /// The default development dataset.
    #[default]
    Dev,
    /// The largest dataset, enough to fill every documented project.
    Demo,
}

impl Profile {
    /// The number of notes the profile seeds.
    const fn notes(self) -> usize {
        match self {
            Self::Minimal => 5,
            Self::Dev => 200,
            Self::Demo => 500,
        }
    }

    /// The number of projects the profile seeds.
    const fn projects(self) -> usize {
        match self {
            Self::Minimal => 1,
            Self::Dev => 8,
            Self::Demo => 12,
        }
    }

    /// The number of tasks the profile seeds.
    const fn tasks(self) -> usize {
        match self {
            Self::Minimal => 3,
            Self::Dev => 60,
            Self::Demo => 120,
        }
    }

    /// The number of expense claims the profile seeds.
    const fn claims(self) -> usize {
        match self {
            Self::Minimal => 1,
            Self::Dev => 25,
            Self::Demo => 50,
        }
    }

    /// The number of expense items the profile seeds.
    ///
    /// It is derived, not a table entry: the sum of [`items_per_claim`] over
    /// the profile's claims. Hard-coding it would let the insert loop and the
    /// count check drift apart, which is the failure `V4` exists to catch.
    ///
    /// `1`, `25` and `50` claims give `0`, `48` and `98` items:
    ///
    /// * `minimal`: claim 0 is the draft with no items, so `0`.
    /// * `dev`: `1 + (i % 3)` over `i = 0..25` is `25 + 24 = 49`, minus the
    ///   first claim's single item, so `48`.
    /// * `demo`: `1 + (i % 3)` over `i = 0..50` is `50 + 49 = 99`, minus the
    ///   first claim's single item, so `98`.
    const fn items(self) -> usize {
        let mut total = 0;
        let mut index = 0;
        while index < self.claims() {
            total += items_per_claim(index);
            index += 1;
        }
        total
    }

    /// The name accepted by `--profile` and printed in the report.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Dev => "dev",
            Self::Demo => "demo",
        }
    }
}

/// The number of items claim `index` carries.
///
/// `1 + (index % 3)` for every claim, except the profile's first claim, which
/// is a `draft` (`index % 5 == 0`) and carries none (`seeding.md` §2.1 and
/// `C-R1` §SEEDER). The insert loop and [`Profile::items`] use this one
/// function, so the seeded rows and the asserted count cannot disagree.
const fn items_per_claim(index: usize) -> usize {
    if index == 0 { 0 } else { 1 + (index % 3) }
}

/// The claim's `created_at`: base + 21 days + `index × 6` hours.
///
/// The item inserts reuse it, because an item's `created_at` and
/// `incurred_on` are derived from its claim's.
fn claim_created_at(index: usize) -> DateTime<Utc> {
    base_timestamp() + Duration::days(21) + Duration::hours(6 * index as i64)
}

/// The parsed command line. It never holds the connection string, so a derived
/// `Debug` cannot leak one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SeedArgs {
    profile: Profile,
    reset: bool,
    allow_non_empty: bool,
    help: bool,
}

impl SeedArgs {
    /// Parses the arguments after the program name.
    ///
    /// # Errors
    ///
    /// Returns a usage message when an argument is unknown, a value is
    /// missing, or the profile name is not one of the three documented values.
    fn parse<I>(arguments: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut parsed = Self {
            profile: Profile::default(),
            reset: false,
            allow_non_empty: false,
            help: false,
        };

        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            if let Some(value) = argument.strip_prefix("--profile=") {
                if value.is_empty() {
                    return Err("--profile requires a value: minimal, dev or demo".to_string());
                }
                parsed.profile = parse_profile(value)?;
                continue;
            }

            match argument.as_str() {
                "--profile" => {
                    let value = arguments.next().ok_or_else(|| {
                        "--profile requires a value: minimal, dev or demo".to_string()
                    })?;
                    if value.starts_with('-') {
                        return Err(format!(
                            "--profile requires a value, but the next argument is {value:?}"
                        ));
                    }
                    parsed.profile = parse_profile(&value)?;
                }
                "--reset" => parsed.reset = true,
                "--allow-non-empty" => parsed.allow_non_empty = true,
                "--help" => parsed.help = true,
                other => {
                    return Err(format!("unknown argument {other:?}; run --help for usage"));
                }
            }
        }

        Ok(parsed)
    }
}

/// Parses a profile name.
///
/// # Errors
///
/// Returns a usage message when the name is not documented.
fn parse_profile(value: &str) -> Result<Profile, String> {
    match value {
        "minimal" => Ok(Profile::Minimal),
        "dev" => Ok(Profile::Dev),
        "demo" => Ok(Profile::Demo),
        other => Err(format!(
            "unknown profile {other:?}; expected minimal, dev or demo"
        )),
    }
}

/// A failure that has already been reduced to safe, fixed text.
///
/// The variant decides the prefix: `refused` for a guard, `error` for anything
/// that went wrong. Neither variant ever holds a URL, a password or a row.
#[derive(Debug)]
enum Failure {
    /// A safety guard refused the run before it could do harm.
    Refused(String),
    /// The run failed.
    Failed(String),
}

impl Failure {
    fn refused(message: impl Into<String>) -> Self {
        Self::Refused(message.into())
    }

    fn failed(message: impl Into<String>) -> Self {
        Self::Failed(message.into())
    }

    /// The complete line the process prints, without the `atom-seed: ` prefix.
    fn render(&self) -> String {
        match self {
            Self::Refused(message) => format!("refused: {message}"),
            Self::Failed(message) => format!("error: {message}"),
        }
    }
}

/// How many rows of one entity were inserted and how many already existed.
#[derive(Debug, Default, Clone, Copy)]
struct EntityOutcome {
    inserted: u64,
    skipped: u64,
}

impl EntityOutcome {
    /// Records the result of one `INSERT ... ON CONFLICT (id) DO NOTHING`.
    fn record(&mut self, rows_affected: u64) {
        self.inserted += rows_affected;
        self.skipped += 1_u64.saturating_sub(rows_affected);
    }
}

/// The per-entity result of a run.
#[derive(Debug, Default, Clone, Copy)]
struct SeedOutcome {
    users: EntityOutcome,
    notes: EntityOutcome,
    projects: EntityOutcome,
    tasks: EntityOutcome,
    claims: EntityOutcome,
    items: EntityOutcome,
}

impl SeedOutcome {
    fn total_inserted(&self) -> u64 {
        self.users.inserted
            + self.notes.inserted
            + self.projects.inserted
            + self.tasks.inserted
            + self.claims.inserted
            + self.items.inserted
    }

    fn total_skipped(&self) -> u64 {
        self.users.skipped
            + self.notes.skipped
            + self.projects.skipped
            + self.tasks.skipped
            + self.claims.skipped
            + self.items.skipped
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let mut arguments = Vec::new();
    for argument in std::env::args_os().skip(1) {
        match argument.into_string() {
            Ok(value) => arguments.push(value),
            Err(_) => {
                eprintln!("atom-seed: usage error: an argument is not valid Unicode");
                return ExitCode::FAILURE;
            }
        }
    }

    let args = match SeedArgs::parse(arguments) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("atom-seed: {message}");
            return ExitCode::FAILURE;
        }
    };

    if args.help {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    match run(args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprintln!("atom-seed: {}", failure.render());
            ExitCode::FAILURE
        }
    }
}

/// Runs the whole operation in the order the contract fixes.
///
/// Every refusal happens before the migration step, so a refused run sends the
/// target no DDL. A failed verification leaves the transaction uncommitted,
/// and the dropped transaction rolls it back.
async fn run(args: SeedArgs) -> Result<(), Failure> {
    // `.env` is optional; a variable already present in the process
    // environment wins, exactly as in `src/main.rs`.
    dotenvy::dotenv().ok();

    // G1: the environment allow-list. It needs no database.
    check_environment()?;

    let database_url = read_database_url()?;

    // G2: the database-name allow-list. It opens nothing.
    let approved_name = approve_database_name(&database_url)?;

    let pool = connect(&database_url).await?;
    confirm_connected_database(&pool, &approved_name).await?;

    let seed_ids = [seeded_id("users", 0), seeded_id("users", 1)];

    // G3 preflight: refuse a database that already holds foreign accounts
    // before any migration is applied.
    let foreign = existing_foreign_accounts(&pool, &seed_ids).await?;
    refuse_foreign_accounts(foreign, args.allow_non_empty)?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|error| {
            Failure::failed(format!(
                "could not apply the migrations: {}",
                describe_migration_error(&error)
            ))
        })?;

    let mut transaction = pool.begin().await.map_err(|error| {
        Failure::failed(format!(
            "could not open a transaction: {}",
            describe_database_error(&error)
        ))
    })?;

    // G3 authoritative: a row that appeared between the preflight and this
    // point is caught here, inside the transaction that would write.
    let foreign = foreign_account_count(&mut transaction, &seed_ids).await?;
    refuse_foreign_accounts(foreign, args.allow_non_empty)?;

    if args.reset {
        println!("{RESET_WARNING}");
        reset_seeded_rows(&mut transaction, &seed_ids).await?;
    }

    // The Argon2 salt is the single permitted non-determinism: the hash is
    // computed once and reused for both accounts.
    let password_hash = hash_seeded_password()?;

    let mut outcome = SeedOutcome {
        users: seed_users(&mut transaction, &password_hash).await?,
        projects: seed_projects(&mut transaction, args.profile).await?,
        tasks: seed_tasks(&mut transaction, args.profile).await?,
        notes: seed_notes(&mut transaction, args.profile).await?,
        ..SeedOutcome::default()
    };

    // Claims come after the projects they may link to; items come after the
    // claims, because the composite key requires the claim to exist.
    let (claims, inserted_claims) = seed_claims(&mut transaction, args.profile).await?;
    outcome.claims = claims;
    outcome.items = seed_items(&mut transaction, args.profile).await?;

    // Every total is set from the items by one statement. No money is added in
    // Rust (ADR-0007); a claim whose insert was skipped keeps its stored total,
    // and `V1` fails if that total no longer matches its items.
    refresh_claim_totals(&mut transaction, &inserted_claims).await?;

    if let Err(failure) = verify(&mut transaction, args.profile, &seed_ids).await {
        transaction.rollback().await.ok();
        return Err(failure);
    }

    transaction.commit().await.map_err(|error| {
        Failure::failed(format!(
            "could not commit the seeded rows: {}",
            describe_database_error(&error)
        ))
    })?;

    print_report(args.profile, &outcome);
    Ok(())
}

/// G1: refuses every `APP_ENV` that is not an explicit development value.
///
/// Unset is allowed. The allow-list is the set `Environment::parse` maps to
/// `Development`; copying it keeps this binary independent of the library
/// crate, and any drift can only cause a refusal, never an admission.
fn check_environment() -> Result<(), Failure> {
    match std::env::var("APP_ENV") {
        Ok(raw) => {
            let normalized = raw.trim().to_ascii_lowercase();
            if matches!(normalized.as_str(), "development" | "dev" | "local") {
                return Ok(());
            }

            Err(Failure::refused(format!(
                "APP_ENV is {normalized:?}, which is not an allowed development environment. \
                 Allowed values are \"development\", \"dev\" and \"local\", or unset. \
                 The seeder writes known credentials and deletes rows. Nothing was written."
            )))
        }
        Err(std::env::VarError::NotPresent) => Ok(()),
        Err(std::env::VarError::NotUnicode(_)) => Err(Failure::refused(
            "APP_ENV is not valid Unicode and cannot be checked. Allowed values are \
             \"development\", \"dev\" and \"local\", or unset. Nothing was written.",
        )),
    }
}

/// Reads `DATABASE_URL` without ever echoing it.
fn read_database_url() -> Result<String, Failure> {
    match std::env::var("DATABASE_URL") {
        Ok(value) => Ok(value),
        Err(std::env::VarError::NotPresent) => Err(Failure::refused(
            "DATABASE_URL is not set. It must name a development database; see \
             docs/database/seeding.md. Nothing was written.",
        )),
        Err(std::env::VarError::NotUnicode(_)) => Err(Failure::refused(
            "DATABASE_URL is not valid Unicode and cannot be used. Nothing was written.",
        )),
    }
}

/// G2: approves the database name through the driver's own parser.
///
/// # Errors
///
/// Returns a refusal when the URL cannot be parsed, names no database, or
/// names a database outside the allow-list. Only the database name is ever
/// rendered, and only after truncation.
fn approve_database_name(database_url: &str) -> Result<String, Failure> {
    let options = PgConnectOptions::from_str(database_url).map_err(|_| {
        Failure::refused(
            "DATABASE_URL could not be parsed as a PostgreSQL connection string, so the \
             database name could not be checked. Nothing was written.",
        )
    })?;

    let name = options.get_database().ok_or_else(|| {
        Failure::refused(
            "DATABASE_URL does not name a database, so the database-name guard cannot \
             approve it. Nothing was written.",
        )
    })?;

    if is_allowed_database_name(name) {
        Ok(name.to_string())
    } else {
        Err(database_name_refusal(name))
    }
}

/// The `G2` allow-list: four exact names plus a `_dev` / `_test` / `_local`
/// suffix rule.
fn is_allowed_database_name(name: &str) -> bool {
    matches!(name, "atom_dev" | "atom_test" | "atom_local" | "atom_demo")
        || name.ends_with("_dev")
        || name.ends_with("_test")
        || name.ends_with("_local")
}

/// The `G2` refusal. The name is shown as a Rust string literal, truncated to
/// 64 characters so an unbounded name cannot flood the terminal.
fn database_name_refusal(name: &str) -> Failure {
    Failure::refused(format!(
        "the database name {:?} is not allowed. Allowed names are \"atom_dev\", \
         \"atom_test\", \"atom_local\" and \"atom_demo\", or a name ending in \"_dev\", \
         \"_test\" or \"_local\". The name is shown as a Rust string literal and truncated \
         to 64 characters. Nothing was written.",
        truncate(name, 64)
    ))
}

/// Cuts a string to at most `limit` characters, on a character boundary.
fn truncate(value: &str, limit: usize) -> &str {
    match value.char_indices().nth(limit) {
        Some((index, _)) => &value[..index],
        None => value,
    }
}

/// Opens a pool with one connection, bounded by [`CONNECT_TIMEOUT`].
async fn connect(database_url: &str) -> Result<PgPool, Failure> {
    let attempt = PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url);

    match tokio::time::timeout(CONNECT_TIMEOUT, attempt).await {
        Ok(Ok(pool)) => Ok(pool),
        Ok(Err(error)) => Err(Failure::failed(format!(
            "could not connect to the database: {}",
            describe_database_error(&error)
        ))),
        Err(_) => Err(Failure::failed(
            "could not connect to the database: the connection timed out. Nothing was written.",
        )),
    }
}

/// Confirms that the connection reports the name `G2` approved.
async fn confirm_connected_database(pool: &PgPool, approved: &str) -> Result<(), Failure> {
    let actual: String = sqlx::query_scalar("SELECT current_database()::text")
        .fetch_one(pool)
        .await
        .map_err(|error| {
            Failure::failed(format!(
                "could not read the connected database name: {}",
                describe_database_error(&error)
            ))
        })?;

    if actual == approved {
        Ok(())
    } else {
        Err(Failure::refused(format!(
            "the connection reports the database {:?}, but the approved name was {:?}. \
             Nothing was written.",
            truncate(&actual, 64),
            truncate(approved, 64)
        )))
    }
}

/// Returns the number of accounts outside the seeded set, if `users` exists.
async fn existing_foreign_accounts(pool: &PgPool, seed_ids: &[Uuid]) -> Result<i64, Failure> {
    let users_exists: bool = sqlx::query_scalar("SELECT to_regclass('users') IS NOT NULL")
        .fetch_one(pool)
        .await
        .map_err(|error| {
            Failure::failed(format!(
                "could not check whether the schema exists: {}",
                describe_database_error(&error)
            ))
        })?;

    if users_exists {
        let mut connection = pool.acquire().await.map_err(|error| {
            Failure::failed(format!(
                "could not acquire a connection: {}",
                describe_database_error(&error)
            ))
        })?;
        foreign_account_count(&mut connection, seed_ids).await
    } else {
        Ok(0)
    }
}

/// Counts the accounts whose id is not one of the two seeded ids.
async fn foreign_account_count(
    connection: &mut PgConnection,
    seed_ids: &[Uuid],
) -> Result<i64, Failure> {
    sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id <> ALL($1)")
        .bind(seed_ids)
        .fetch_one(connection)
        .await
        .map_err(|error| {
            Failure::failed(format!(
                "could not count the accounts outside the seeded set: {}",
                describe_database_error(&error)
            ))
        })
}

/// Applies the `G3` rule to a count.
fn refuse_foreign_accounts(foreign: i64, allow_non_empty: bool) -> Result<(), Failure> {
    if foreign <= 0 || allow_non_empty {
        return Ok(());
    }

    let noun = if foreign == 1 { "account" } else { "accounts" };
    Err(Failure::refused(format!(
        "the database holds {foreign} {noun} outside the seeded set. Pass \
         --allow-non-empty to admit it. --reset deletes every row owned by the two \
         seeded accounts and leaves the other accounts alone. No data rows were written."
    )))
}

/// Deletes every row owned by the two seeded accounts, in foreign-key order.
///
/// Each statement is static and each predicate is `user_id = ANY($1)` or a
/// subquery over it. `tasks` is reached through `projects` because the table
/// has no `user_id` column.
async fn reset_seeded_rows(
    connection: &mut PgConnection,
    seed_ids: &[Uuid],
) -> Result<(), Failure> {
    let statements = [
        (
            "expense_items",
            "DELETE FROM expense_items WHERE user_id = ANY($1)",
        ),
        (
            "expense_claims",
            "DELETE FROM expense_claims WHERE user_id = ANY($1)",
        ),
        (
            "refresh_sessions",
            "DELETE FROM refresh_sessions WHERE user_id = ANY($1)",
        ),
        ("notes", "DELETE FROM notes WHERE user_id = ANY($1)"),
        (
            "tasks",
            "DELETE FROM tasks \
             WHERE project_id IN (SELECT id FROM projects WHERE user_id = ANY($1))",
        ),
        ("projects", "DELETE FROM projects WHERE user_id = ANY($1)"),
        ("users", "DELETE FROM users WHERE id = ANY($1)"),
    ];

    for (entity, sql) in statements {
        sqlx::query(sql)
            .bind(seed_ids)
            .execute(&mut *connection)
            .await
            .map_err(|error| {
                Failure::failed(format!(
                    "could not delete the seeded {entity}: {}",
                    describe_database_error(&error)
                ))
            })?;
    }

    Ok(())
}

/// Hashes the published development password once.
///
/// The salt is random and is the one deliberately non-deterministic value in
/// the run. The hash itself is never printed.
fn hash_seeded_password() -> Result<String, Failure> {
    let argon2 = build_argon2()?;

    let mut salt_bytes = [0_u8; 16];
    rand::rng().fill_bytes(&mut salt_bytes);
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|_| Failure::failed("could not encode the Argon2 salt"))?;

    argon2
        .hash_password(SEEDED_PASSWORD.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| Failure::failed("could not hash the development password"))
}

/// Builds the Argon2id instance from the constants above.
fn build_argon2() -> Result<Argon2<'static>, Failure> {
    let params = Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_PARALLELISM,
        None,
    )
    .map_err(|_| Failure::failed("could not build the Argon2 parameters"))?;

    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

/// Derives a deterministic UUID for an entity row.
///
/// `digest = SHA-256("atom-seed/v1/<entity>/<index>")`; the first 16 bytes
/// become a UUID with the RFC 4122 version-4 and variant bits set. The
/// derivation is part of the contract: changing it orphans the old rows.
fn seeded_id(entity: &str, index: usize) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(b"atom-seed/v1/");
    hasher.update(entity.as_bytes());
    hasher.update(b"/");
    hasher.update(index.to_string().as_bytes());
    let digest = hasher.finalize();

    let mut bytes = [0_u8; 16];
    if let Some(head) = digest.get(..16) {
        bytes.copy_from_slice(head);
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    Uuid::from_bytes(bytes)
}

/// The fixed `2026-01-01T00:00:00Z` base every timestamp derives from.
fn base_timestamp() -> DateTime<Utc> {
    // 2026-01-01T00:00:00Z as a Unix timestamp.
    const BASE_UNIX_SECONDS: i64 = 1_767_225_600;
    // The constant is a valid instant, so the fallback is unreachable; it
    // keeps the function total without `unwrap`.
    DateTime::<Utc>::from_timestamp(BASE_UNIX_SECONDS, 0).unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
}

/// Inserts the two seeded accounts.
async fn seed_users(
    connection: &mut PgConnection,
    password_hash: &str,
) -> Result<EntityOutcome, Failure> {
    let mut outcome = EntityOutcome::default();
    let base = base_timestamp();

    for (index, (email, display_name)) in SEEDED_USERS.iter().enumerate() {
        let id = seeded_id("users", index);
        let created_at = base + Duration::days(index as i64);
        let updated_at = created_at + Duration::hours(1);

        let inserted = sqlx::query(
            "INSERT INTO users \
                 (id, email, password_hash, display_name, is_active, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(id)
        .bind(*email)
        .bind(password_hash)
        .bind(*display_name)
        .bind(true)
        .bind(created_at)
        .bind(updated_at)
        .execute(&mut *connection)
        .await
        .map_err(|error| insert_failure("users", &error))?
        .rows_affected();

        outcome.record(inserted);
    }

    Ok(outcome)
}

/// Inserts the profile's projects.
async fn seed_projects(
    connection: &mut PgConnection,
    profile: Profile,
) -> Result<EntityOutcome, Failure> {
    let mut outcome = EntityOutcome::default();
    let base = base_timestamp();

    for index in 0..profile.projects() {
        let (name, description) = PROJECTS[index % PROJECTS.len()];
        let id = seeded_id("projects", index);
        let user_id = seeded_id("users", index % SEEDED_USERS.len());
        let created_at = base + Duration::days(7 + index as i64);
        let updated_at = created_at + Duration::hours(1);

        let inserted = sqlx::query(
            "INSERT INTO projects \
                 (id, user_id, name, description, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(id)
        .bind(user_id)
        .bind(name)
        .bind(description)
        .bind(created_at)
        .bind(updated_at)
        .execute(&mut *connection)
        .await
        .map_err(|error| insert_failure("projects", &error))?
        .rows_affected();

        outcome.record(inserted);
    }

    Ok(outcome)
}

/// Inserts the profile's tasks. A task inherits its owner through its project.
async fn seed_tasks(
    connection: &mut PgConnection,
    profile: Profile,
) -> Result<EntityOutcome, Failure> {
    let mut outcome = EntityOutcome::default();
    let base = base_timestamp();

    let project_count = profile.projects();
    if project_count == 0 {
        return Err(Failure::failed(
            "the selected profile has no projects, so no task can be attached",
        ));
    }

    for index in 0..profile.tasks() {
        let title = TASK_TITLES[index % TASK_TITLES.len()];
        let detail = TASK_DETAILS[index % TASK_DETAILS.len()];
        let id = seeded_id("tasks", index);
        let project_id = seeded_id("projects", index % project_count);
        let status = match index % 3 {
            0 => "open",
            1 => "in_progress",
            _ => "done",
        };
        let priority = match index % 4 {
            0 => "low",
            1 => "medium",
            2 => "high",
            _ => "urgent",
        };
        // The integer division is performed in integers and only the final
        // value is converted for the DOUBLE PRECISION column.
        let position = (index / project_count) as f64;
        let created_at = base + Duration::days(14) + Duration::minutes(20 * index as i64);
        let completed_at = if status == "done" {
            Some(created_at + Duration::hours(2))
        } else {
            None
        };
        let updated_at = match completed_at {
            Some(value) => value + Duration::hours(1),
            None => created_at + Duration::hours(1),
        };
        let description = format!(
            "{detail} Seeded task {} for the {} profile.",
            index + 1,
            profile.as_str()
        );

        let inserted = sqlx::query(
            "INSERT INTO tasks \
                 (id, project_id, title, description, status, priority, position, created_at, \
                  updated_at, completed_at) \
             VALUES ($1, $2, $3, $4, $5::text::task_status, $6::text::task_priority, $7, \
                     $8, $9, $10) \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(id)
        .bind(project_id)
        .bind(title)
        .bind(description)
        .bind(status)
        .bind(priority)
        .bind(position)
        .bind(created_at)
        .bind(updated_at)
        .bind(completed_at)
        .execute(&mut *connection)
        .await
        .map_err(|error| insert_failure("tasks", &error))?
        .rows_affected();

        outcome.record(inserted);
    }

    Ok(outcome)
}

/// Inserts the profile's notes.
async fn seed_notes(
    connection: &mut PgConnection,
    profile: Profile,
) -> Result<EntityOutcome, Failure> {
    let mut outcome = EntityOutcome::default();
    let base = base_timestamp();

    for index in 0..profile.notes() {
        let (topic, body) = NOTES[index % NOTES.len()];
        let id = seeded_id("notes", index);
        let user_id = seeded_id("users", index % SEEDED_USERS.len());
        let title = format!("{topic} ({})", index + 1);
        let content = format!("{body}\n\nSeed reference: atom-seed/v1/notes/{index}.");
        let is_archived = index % 7 == 6;
        let created_at = base + Duration::days(1) + Duration::minutes(30 * index as i64);
        let updated_at = created_at + Duration::hours(1);

        let inserted = sqlx::query(
            "INSERT INTO notes \
                 (id, user_id, title, content, is_archived, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(id)
        .bind(user_id)
        .bind(title)
        .bind(content)
        .bind(is_archived)
        .bind(created_at)
        .bind(updated_at)
        .execute(&mut *connection)
        .await
        .map_err(|error| insert_failure("notes", &error))?
        .rows_affected();

        outcome.record(inserted);
    }

    Ok(outcome)
}

/// Inserts the profile's expense claims, returning the ids this run wrote.
///
/// The returned ids are the ones the total refresh touches: a claim whose
/// insert was skipped keeps its stored total, which `V1` then verifies.
///
/// The lifecycle timestamps are set from `index % 5` so that
/// `expense_claims_lifecycle_timestamps_chk` holds for every status: `draft`
/// has none, `submitted` has `submitted_at`, `approved` and `rejected` add
/// `decided_at`, and `paid` adds `paid_at`. The order is the one
/// `expense_claims_timestamp_order_chk` requires.
///
/// `total_amount` is written as `0` and replaced from `SUM(amount)` after the
/// items exist. No money is computed in Rust.
async fn seed_claims(
    connection: &mut PgConnection,
    profile: Profile,
) -> Result<(EntityOutcome, Vec<Uuid>), Failure> {
    let mut outcome = EntityOutcome::default();
    let mut inserted_ids: Vec<Uuid> = Vec::new();
    let project_count = profile.projects();

    for index in 0..profile.claims() {
        let id = seeded_id("expense_claims", index);
        let user_id = seeded_id("users", index % SEEDED_USERS.len());
        let title = CLAIM_TITLES[index % CLAIM_TITLES.len()];
        let description = format!(
            "{} Seeded claim {} for the {} profile.",
            CLAIM_DESCRIPTIONS[index % CLAIM_DESCRIPTIONS.len()],
            index + 1,
            profile.as_str()
        );
        let currency = CLAIM_CURRENCIES[index % CLAIM_CURRENCIES.len()];
        let created_at = claim_created_at(index);
        let submitted_at_candidate = created_at + Duration::days(1);
        let decided_at_candidate = created_at + Duration::days(2);
        let paid_at_candidate = created_at + Duration::days(3);

        let (status, submitted_at, decided_at, paid_at) = match index % 5 {
            0 => ("draft", None, None, None),
            1 => ("submitted", Some(submitted_at_candidate), None, None),
            2 => (
                "approved",
                Some(submitted_at_candidate),
                Some(decided_at_candidate),
                None,
            ),
            3 => (
                "rejected",
                Some(submitted_at_candidate),
                Some(decided_at_candidate),
                None,
            ),
            _ => (
                "paid",
                Some(submitted_at_candidate),
                Some(decided_at_candidate),
                Some(paid_at_candidate),
            ),
        };

        // `updated_at` is the latest state change plus one hour; for a draft,
        // whose state has not changed, it is its creation plus one hour.
        let last_state = paid_at
            .or(decided_at)
            .or(submitted_at)
            .unwrap_or(created_at);
        let updated_at = last_state + Duration::hours(1);

        // Claim `i` links to project `i % project_count`. Every profile has at
        // least one project; the `None` arm keeps the rule total.
        let project_id = if project_count == 0 {
            None
        } else {
            Some(seeded_id("projects", index % project_count))
        };

        let inserted = sqlx::query(
            "INSERT INTO expense_claims \
                 (id, user_id, title, description, currency, total_amount, status, \
                  project_id, submitted_at, decided_at, paid_at, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5::char(3), 0, $6::text::expense_claim_status, $7, \
                     $8, $9, $10, $11, $12) \
             ON CONFLICT (id) DO NOTHING \
             RETURNING id",
        )
        .bind(id)
        .bind(user_id)
        .bind(title)
        .bind(description)
        .bind(currency)
        .bind(status)
        .bind(project_id)
        .bind(submitted_at)
        .bind(decided_at)
        .bind(paid_at)
        .bind(created_at)
        .bind(updated_at)
        .fetch_optional(&mut *connection)
        .await
        .map_err(|error| insert_failure("expense_claims", &error))?;

        match inserted {
            Some(_) => {
                outcome.record(1);
                inserted_ids.push(id);
            }
            None => outcome.record(0),
        }
    }

    Ok((outcome, inserted_ids))
}

/// Inserts the profile's expense items.
///
/// Item ids are `SHA-256("atom-seed/v1/expense_items/<n>")` with `n` counting
/// across the whole profile, so the derivation does not depend on how the
/// claims are grouped. The owner is the claim's owner, which the composite key
/// would reject otherwise. `amount` is a fixed decimal string bound and cast
/// to `numeric`, and `incurred_on` derives from the claim's `created_at`.
async fn seed_items(
    connection: &mut PgConnection,
    profile: Profile,
) -> Result<EntityOutcome, Failure> {
    let mut outcome = EntityOutcome::default();
    let mut item_index = 0_usize;

    for claim_index in 0..profile.claims() {
        let claim_id = seeded_id("expense_claims", claim_index);
        let user_id = seeded_id("users", claim_index % SEEDED_USERS.len());
        let claim_created = claim_created_at(claim_index);
        let incurred_on = (claim_created - Duration::days(3)).date_naive();

        for line in 0..items_per_claim(claim_index) {
            let id = seeded_id("expense_items", item_index);
            let base = ITEM_DESCRIPTIONS[item_index % ITEM_DESCRIPTIONS.len()];
            let description = format!("{} (line {})", base, line + 1);
            let amount = ITEM_AMOUNTS[item_index % ITEM_AMOUNTS.len()];
            let category = ITEM_CATEGORIES[item_index % ITEM_CATEGORIES.len()];
            let created_at = claim_created + Duration::hours((line + 1) as i64);
            let updated_at = created_at + Duration::hours(1);

            // Some rows carry a receipt and some do not, so both branches of
            // `expense_items_receipt_url_chk` have data behind them.
            let receipt_url = if item_index % 3 == 0 {
                Some(format!(
                    "https://receipts.atom.local/atom-seed/expense-items/{item_index}.pdf"
                ))
            } else {
                None
            };

            let inserted = sqlx::query(
                "INSERT INTO expense_items \
                     (id, user_id, claim_id, description, amount, incurred_on, category, \
                      receipt_url, created_at, updated_at) \
                 VALUES ($1, $2, $3, $4, $5::numeric, $6, $7, $8, $9, $10) \
                 ON CONFLICT (id) DO NOTHING",
            )
            .bind(id)
            .bind(user_id)
            .bind(claim_id)
            .bind(description)
            .bind(amount)
            .bind(incurred_on)
            .bind(category)
            .bind(receipt_url)
            .bind(created_at)
            .bind(updated_at)
            .execute(&mut *connection)
            .await
            .map_err(|error| insert_failure("expense_items", &error))?
            .rows_affected();

            outcome.record(inserted);
            item_index += 1;
        }
    }

    Ok(outcome)
}

/// Sets every newly inserted claim's `total_amount` from its items.
///
/// One static statement, so the sum is computed by PostgreSQL over the exact
/// stored decimals and the result is stored at the column's own scale. A claim
/// with no items gets `COALESCE(..., 0)`, which the column renders `0.0000`.
async fn refresh_claim_totals(
    connection: &mut PgConnection,
    claim_ids: &[Uuid],
) -> Result<(), Failure> {
    if claim_ids.is_empty() {
        return Ok(());
    }

    sqlx::query(
        "UPDATE expense_claims AS c \
         SET total_amount = COALESCE( \
                 (SELECT SUM(i.amount) FROM expense_items i \
                  WHERE i.claim_id = c.id AND i.user_id = c.user_id), \
                 0) \
         WHERE c.id = ANY($1)",
    )
    .bind(claim_ids)
    .execute(&mut *connection)
    .await
    .map_err(|error| {
        Failure::failed(format!(
            "could not set the claim totals from their items: {}",
            describe_database_error(&error)
        ))
    })?;

    Ok(())
}

/// Runs the six post-insert checks before the transaction commits.
async fn verify(
    connection: &mut PgConnection,
    profile: Profile,
    seed_ids: &[Uuid],
) -> Result<(), Failure> {
    verify_claim_totals(connection, profile).await?;
    verify_ownership(connection, profile, seed_ids).await?;
    verify_completion_timestamps(connection, profile).await?;
    verify_counts(connection, profile, seed_ids).await?;
    verify_documented_credentials(connection, &seed_ids[0]).await?;
    verify_table_list(connection).await
}

/// `V1`: every seeded claim's total is the sum of its own items.
///
/// The comparison is made by PostgreSQL, over the stored `NUMERIC(19,4)`
/// values; nothing in Rust sees a number. A claim with no items must carry
/// `0.0000`, which `COALESCE(..., 0)` produces.
async fn verify_claim_totals(
    connection: &mut PgConnection,
    profile: Profile,
) -> Result<(), Failure> {
    let claim_ids = seeded_ids("expense_claims", profile.claims());

    let mismatched = query_count_single(
        connection,
        "SELECT COUNT(*) FROM expense_claims c \
         WHERE c.id = ANY($1) \
           AND c.total_amount <> COALESCE( \
                   (SELECT SUM(i.amount) FROM expense_items i \
                    WHERE i.claim_id = c.id AND i.user_id = c.user_id), \
                   0)",
        &claim_ids,
    )
    .await?;

    expect_zero(
        "V1 claim totals",
        mismatched,
        "a seeded claim's total_amount differs from the sum of its items",
    )
}

/// `V2`: every seeded row belongs to one of the two seeded accounts.
///
/// The check is restricted to the deterministic ids, so rows owned by accounts
/// outside the seeded set can never make an `--allow-non-empty` run fail.
async fn verify_ownership(
    connection: &mut PgConnection,
    profile: Profile,
    seed_ids: &[Uuid],
) -> Result<(), Failure> {
    let note_ids = seeded_ids("notes", profile.notes());
    let project_ids = seeded_ids("projects", profile.projects());
    let task_ids = seeded_ids("tasks", profile.tasks());

    let notes = query_count(
        connection,
        "SELECT COUNT(*) FROM notes WHERE id = ANY($1) AND user_id <> ALL($2)",
        &note_ids,
        seed_ids,
    )
    .await?;
    expect_zero(
        "V2 ownership (notes)",
        notes,
        "seeded notes belong to a user outside the seeded set",
    )?;

    let projects = query_count(
        connection,
        "SELECT COUNT(*) FROM projects WHERE id = ANY($1) AND user_id <> ALL($2)",
        &project_ids,
        seed_ids,
    )
    .await?;
    expect_zero(
        "V2 ownership (projects)",
        projects,
        "seeded projects belong to a user outside the seeded set",
    )?;

    let tasks = query_count(
        connection,
        "SELECT COUNT(*) FROM tasks t JOIN projects p ON p.id = t.project_id \
         WHERE t.id = ANY($1) AND p.user_id <> ALL($2)",
        &task_ids,
        seed_ids,
    )
    .await?;
    expect_zero(
        "V2 ownership (tasks)",
        tasks,
        "seeded tasks belong to projects owned outside the seeded set",
    )?;

    let claim_ids = seeded_ids("expense_claims", profile.claims());
    let claims = query_count(
        connection,
        "SELECT COUNT(*) FROM expense_claims WHERE id = ANY($1) AND user_id <> ALL($2)",
        &claim_ids,
        seed_ids,
    )
    .await?;
    expect_zero(
        "V2 ownership (claims)",
        claims,
        "seeded claims belong to a user outside the seeded set",
    )?;

    let claim_projects = query_count_single(
        connection,
        "SELECT COUNT(*) FROM expense_claims c JOIN projects p ON p.id = c.project_id \
         WHERE c.id = ANY($1) AND p.user_id <> c.user_id",
        &claim_ids,
    )
    .await?;
    expect_zero(
        "V2 ownership (claim projects)",
        claim_projects,
        "a seeded claim is linked to a project owned by another account",
    )?;

    let item_ids = seeded_ids("expense_items", profile.items());
    let items = query_count(
        connection,
        "SELECT COUNT(*) FROM expense_items WHERE id = ANY($1) AND user_id <> ALL($2)",
        &item_ids,
        seed_ids,
    )
    .await?;
    expect_zero(
        "V2 ownership (items)",
        items,
        "seeded items belong to a user outside the seeded set",
    )?;

    let item_owners = query_count_single(
        connection,
        "SELECT COUNT(*) FROM expense_items i JOIN expense_claims c ON c.id = i.claim_id \
         WHERE i.id = ANY($1) AND i.user_id <> c.user_id",
        &item_ids,
    )
    .await?;
    expect_zero(
        "V2 ownership (item claims)",
        item_owners,
        "a seeded item disagrees with its claim about the owner",
    )?;

    Ok(())
}

/// `V3`: the completion timestamp agrees with the status on every seeded task.
async fn verify_completion_timestamps(
    connection: &mut PgConnection,
    profile: Profile,
) -> Result<(), Failure> {
    let task_ids = seeded_ids("tasks", profile.tasks());
    let inconsistent: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tasks \
         WHERE id = ANY($1) AND ((status = 'done') <> (completed_at IS NOT NULL))",
    )
    .bind(&task_ids)
    .fetch_one(&mut *connection)
    .await
    .map_err(|error| {
        Failure::failed(format!(
            "could not verify the completion timestamps: {}",
            describe_database_error(&error)
        ))
    })?;

    expect_zero(
        "V3 completion consistency",
        inconsistent,
        "seeded tasks have a status that disagrees with completed_at",
    )
}

/// `V4`: the seeded rows match the profile exactly.
async fn verify_counts(
    connection: &mut PgConnection,
    profile: Profile,
    seed_ids: &[Uuid],
) -> Result<(), Failure> {
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id = ANY($1)")
        .bind(seed_ids)
        .fetch_one(&mut *connection)
        .await
        .map_err(|error| {
            Failure::failed(format!(
                "could not count the seeded users: {}",
                describe_database_error(&error)
            ))
        })?;
    expect_count("V4 counts (users)", SEEDED_USERS.len(), users)?;

    let notes = query_count_single(
        connection,
        "SELECT COUNT(*) FROM notes WHERE user_id = ANY($1)",
        seed_ids,
    )
    .await?;
    expect_count("V4 counts (notes)", profile.notes(), notes)?;

    let projects = query_count_single(
        connection,
        "SELECT COUNT(*) FROM projects WHERE user_id = ANY($1)",
        seed_ids,
    )
    .await?;
    expect_count("V4 counts (projects)", profile.projects(), projects)?;

    let tasks = query_count_single(
        connection,
        "SELECT COUNT(*) FROM tasks t JOIN projects p ON p.id = t.project_id \
         WHERE p.user_id = ANY($1)",
        seed_ids,
    )
    .await?;
    expect_count("V4 counts (tasks)", profile.tasks(), tasks)?;

    let claims = query_count_single(
        connection,
        "SELECT COUNT(*) FROM expense_claims WHERE user_id = ANY($1)",
        seed_ids,
    )
    .await?;
    expect_count("V4 counts (claims)", profile.claims(), claims)?;

    let items = query_count_single(
        connection,
        "SELECT COUNT(*) FROM expense_items WHERE user_id = ANY($1)",
        seed_ids,
    )
    .await?;
    expect_count("V4 counts (items)", profile.items(), items)?;

    Ok(())
}

/// `V5`: the published password still verifies against the stored hash.
async fn verify_documented_credentials(
    connection: &mut PgConnection,
    owner_id: &Uuid,
) -> Result<(), Failure> {
    let stored: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
            .bind(*owner_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(|error| {
                Failure::failed(format!(
                    "could not read the seeded credential: {}",
                    describe_database_error(&error)
                ))
            })?;

    let stored = stored.ok_or_else(|| {
        verification_failure(
            "V5 credentials: the owner row is missing after the insert; use --reset to restore it",
        )
    })?;

    let parsed = PasswordHash::new(&stored).map_err(|_| {
        verification_failure(
            "V5 credentials: the stored hash is not a valid PHC string; use --reset to restore it",
        )
    })?;

    build_argon2()?
        .verify_password(SEEDED_PASSWORD.as_bytes(), &parsed)
        .map_err(|_| {
            verification_failure(
                "V5 credentials: the documented password does not verify against the stored \
                 hash for owner@atom.local; use --reset to restore it",
            )
        })
}

/// `V6`: the delete list names every user table that exists.
async fn verify_table_list(connection: &mut PgConnection) -> Result<(), Failure> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name::text FROM information_schema.tables \
         WHERE table_schema = 'public' AND table_type = 'BASE TABLE' \
           AND table_name <> '_sqlx_migrations' \
         ORDER BY table_name",
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(|error| {
        Failure::failed(format!(
            "could not read the schema table list: {}",
            describe_database_error(&error)
        ))
    })?;

    for table in tables {
        if !SEEDED_TABLES.contains(&table.as_str()) {
            return Err(verification_failure(format!(
                "V6 delete-list coverage: the table {table:?} is not in the seeder's table list"
            )));
        }
    }

    Ok(())
}

/// Builds the deterministic ids of an entity's first `count` rows.
fn seeded_ids(entity: &str, count: usize) -> Vec<Uuid> {
    (0..count).map(|index| seeded_id(entity, index)).collect()
}

/// Runs a count statement that binds the deterministic ids and the seeded
/// accounts, and returns the count.
async fn query_count(
    connection: &mut PgConnection,
    sql: &str,
    ids: &[Uuid],
    seed_ids: &[Uuid],
) -> Result<i64, Failure> {
    sqlx::query_scalar(sql)
        .bind(ids)
        .bind(seed_ids)
        .fetch_one(&mut *connection)
        .await
        .map_err(|error| {
            Failure::failed(format!(
                "could not run a verification count: {}",
                describe_database_error(&error)
            ))
        })
}

/// Runs a count statement that binds only one parameter, and returns the
/// count.
async fn query_count_single(
    connection: &mut PgConnection,
    sql: &str,
    ids: &[Uuid],
) -> Result<i64, Failure> {
    sqlx::query_scalar(sql)
        .bind(ids)
        .fetch_one(&mut *connection)
        .await
        .map_err(|error| {
            Failure::failed(format!(
                "could not run a verification count: {}",
                describe_database_error(&error)
            ))
        })
}

/// Fails when a verification count is not zero.
fn expect_zero(check: &str, actual: i64, description: &str) -> Result<(), Failure> {
    if actual == 0 {
        Ok(())
    } else {
        Err(verification_failure(format!(
            "{check}: expected 0, found {actual}; {description}"
        )))
    }
}

/// Fails when a verification count differs from the profile.
fn expect_count(check: &str, expected: usize, actual: i64) -> Result<(), Failure> {
    let expected = expected as i64;
    if actual == expected {
        Ok(())
    } else {
        Err(verification_failure(format!(
            "{check}: expected {expected}, found {actual}"
        )))
    }
}

/// Builds a transaction-rollback verification failure.
fn verification_failure(detail: impl Into<String>) -> Failure {
    Failure::failed(format!(
        "verification failed: {}. The transaction was rolled back; the database is unchanged.",
        detail.into()
    ))
}

/// Builds the insert failure for one entity.
fn insert_failure(entity: &str, error: &sqlx::Error) -> Failure {
    Failure::failed(format!(
        "could not insert into {entity}: {}",
        describe_database_error(error)
    ))
}

/// Prints the short success summary. It names entities, integers and the two
/// documented emails; it never prints a hash, the URL or the password.
fn print_report(profile: Profile, outcome: &SeedOutcome) {
    println!("atom-seed: profile {}", profile.as_str());
    print_entity("users", outcome.users);
    print_entity("notes", outcome.notes);
    print_entity("projects", outcome.projects);
    print_entity("tasks", outcome.tasks);
    print_entity("claims", outcome.claims);
    print_entity("items", outcome.items);
    println!(
        "atom-seed: verification passed (6 checks); {} rows inserted, {} skipped",
        outcome.total_inserted(),
        outcome.total_skipped()
    );
    println!("atom-seed: accounts owner@atom.local and other@atom.local");
}

/// Prints one entity's inserted and skipped counts.
fn print_entity(name: &str, outcome: EntityOutcome) {
    println!(
        "  {name:<10} inserted {:<5} skipped {}",
        outcome.inserted, outcome.skipped
    );
}

/// Describes a database failure without revealing the driver message.
///
/// This is the model from `src/main.rs`: the driver message can contain the
/// connection string or a row, so only the error kind and the SQLSTATE are
/// reported.
fn describe_database_error(error: &sqlx::Error) -> String {
    match error.as_database_error() {
        Some(database) => match database.code() {
            Some(code) => format!("{:?} (SQLSTATE {code})", database.kind()),
            None => format!("{:?}", database.kind()),
        },
        // Every other variant carries a message the driver wrote, and a
        // configuration error can carry the connection string, so the message
        // is not repeated.
        None => match error {
            sqlx::Error::Io(inner) => format!("the connection could not be established: {inner}"),
            _ => "the connection could not be established".to_string(),
        },
    }
}

/// Describes a migration failure without revealing the driver message.
///
/// Unlike `describe_migration_error` in `src/main.rs`, the wildcard arm does
/// not repeat the error's `Display`, because that display can carry a driver
/// message (`A-C-S1-8`).
fn describe_migration_error(error: &sqlx::migrate::MigrateError) -> String {
    match error {
        sqlx::migrate::MigrateError::Execute(inner) => describe_database_error(inner),
        sqlx::migrate::MigrateError::ExecuteMigration(inner, version) => {
            format!(
                "migration {version} failed: {}",
                describe_database_error(inner)
            )
        }
        _ => "a migration could not be applied".to_string(),
    }
}

/// Project names and one-paragraph descriptions.
const PROJECTS: [(&str, &str); 12] = [
    (
        "ATOM API",
        "Core HTTP surface. Routing, extractors, error envelopes and the OpenAPI document. \
         This project tracks the handler and contract work that the rest of the workspace \
         depends on.",
    ),
    (
        "Auth Hardening",
        "Authentication and token lifecycle. Argon2id parameters, refresh rotation, reuse \
         detection, the rate limiter and the session pruning task.",
    ),
    (
        "Notes Search",
        "PostgreSQL full-text search over notes. The simple text configuration, weighted \
         tsvector generation and the GIN index that serves the search endpoint.",
    ),
    (
        "Task Board",
        "Task ordering and lifecycle. The task_status and task_priority enum types, the \
         completion consistency constraint and the per-project position index.",
    ),
    (
        "Reimbursements",
        "Expense claims and their items. Accepted for Phase 2; the schema and the money \
         representation are recorded in the decision records, not built yet.",
    ),
    (
        "Job Search",
        "Job applications and their events. Accepted for Phase 2 as a personal pipeline with \
         no generated text and no external service.",
    ),
    (
        "Local ML",
        "The local inference workspace. Burn with the pure-Rust flex backend, model weights \
         stored as SafeTensors, and no network call in the request path.",
    ),
    (
        "Release Engineering",
        "Build, container and release mechanics. Reproducible builds, the vendored OpenAPI \
         explorer, the migration policy and the version prefix.",
    ),
    (
        "Developer Experience",
        "Local setup, the disposable test database, migration policy and the documentation \
         checker. Everything a contributor needs before the first pull request.",
    ),
    (
        "Observability",
        "Structured tracing, request identifiers, health probes and the log filter. The \
         request timeout and body-size backstop live here.",
    ),
    (
        "Data Model",
        "The PostgreSQL schema and its invariants. UUID primary keys, timestamptz columns, \
         ownership foreign keys and the append-only migration rule.",
    ),
    (
        "Documentation",
        "The project knowledge base. Product truth, feature specifications, architecture \
         notes, decision records and the run ledger.",
    ),
];

/// Note titles and bodies. Bodies contain identifiers and versions so that
/// search has realistic material to match.
const NOTES: [(&str, &str); 10] = [
    (
        "Rust ownership in the request path",
        "The request path moves the payload into the service and borrows it for the repository \
         call. Keeping the ownership boundary at the service means the handler never clones a \
         body and the repository never holds a reference across an await.",
    ),
    (
        "Axum extractor ordering",
        "axum 0.8 runs extractors left to right, and only the last one may consume the body. A \
         Json<T> extractor must therefore follow the authenticated user and the path \
         parameters. A rejected extractor still produces the standard error envelope.",
    ),
    (
        "SQLx migrations and the offline cache",
        "sqlx 0.8 embeds migrations with sqlx::migrate! and records each applied version in \
         _sqlx_migrations. The directory is append-only: an applied migration is never edited, \
         and a schema change ships with the matching model change.",
    ),
    (
        "Argon2 parameters and the PHC string",
        "argon2 0.5 stores memory, iterations, parallelism and salt inside the PHC string, so a \
         stored hash keeps verifying after config/auth.toml changes. The cost is raised \
         deliberately; the 8192 KiB floor is a security bound, not a preference.",
    ),
    (
        "PostgreSQL index choices for notes",
        "The notes table carries a composite index on (user_id, created_at DESC), an archived \
         filter index and a GIN index over the weighted tsvector. The simple text configuration \
         keeps identifiers such as UUIDv4 and RFC 3339 searchable without stemming.",
    ),
    (
        "Refresh token rotation",
        "Each login writes a refresh_sessions row holding only the SHA-256 hash of the token. \
         Exchanging the token marks the old row revoked and points the new row at it through \
         replaced_by_session_id, so reuse of a rotated token is detectable.",
    ),
    (
        "Clippy in continuous integration",
        "CI runs cargo clippy --all-targets --all-features --locked -- -D warnings, and a \
         warning fails the build. Suppression attributes are not accepted as a fix; the code \
         changes instead. cargo fmt --all runs before clippy.",
    ),
    (
        "The error envelope",
        "Every failure is an AppResult<T> and serialises to the same JSON shape. Internal \
         details are redacted: a database error reports its SQLSTATE, never the driver \
         message, and an unowned resource answers HTTP 404 rather than 403.",
    ),
    (
        "Pagination limits",
        "List endpoints accept limit and offset, clamp the limit and return a stable order. \
         The published defaults and bounds are part of the client contract, so changing them \
         is a breaking change rather than a tuning exercise.",
    ),
    (
        "Windows development notes",
        "The test database is disposable and its name must contain test. PowerShell sets \
         TEST_DATABASE_URL before cargo test, and the integration harness refuses any other \
         database. Docker remains optional for local development.",
    ),
];

/// Task titles and the detail sentence appended to each description.
const TASK_TITLES: [&str; 10] = [
    "Write the migration",
    "Add the handler test",
    "Review the error envelope",
    "Check the query plan",
    "Document the endpoint",
    "Verify the ownership predicate",
    "Run the release build",
    "Profile the list endpoint",
    "Update the change log",
    "Re-read the contract",
];

/// The detail sentence for a seeded task.
const TASK_DETAILS: [&str; 10] = [
    "Keep the migration append-only and pair it with the model change.",
    "Cover the success path, the validation failure and the unowned resource.",
    "Confirm the envelope matches every other endpoint.",
    "Read EXPLAIN ANALYZE before adding an index.",
    "Publish the status code, the request body and the response schema.",
    "A resource the caller does not own must answer 404, never 403.",
    "Check that the optimized artifact still passes the integration suite.",
    "Measure before and after; do not tune by intuition.",
    "Record the decision and the verification evidence together.",
    "The ledger wins over the prose summary where they disagree.",
];

/// Claim titles. Each one is short enough for the 200-character column check.
const CLAIM_TITLES: [&str; 10] = [
    "Conference travel",
    "Client dinner",
    "Team offsite",
    "Hardware purchase",
    "Software subscription",
    "Rail tickets",
    "Hotel stay",
    "Training course",
    "Home office chair",
    "Courier charges",
];

/// One sentence appended to each claim's title to form its description.
const CLAIM_DESCRIPTIONS: [&str; 5] = [
    "Reimbursement for travel booked for the working group.",
    "Expenses recorded against an approved project budget.",
    "Receipts attached where the vendor supplied one.",
    "Advance paid personally and claimed after the trip.",
    "Recurring cost renewed for the current quarter.",
];

/// The currencies the claim rows cycle through. Each is three upper-case
/// letters, as `expense_claims_currency_chk` requires.
const CLAIM_CURRENCIES: [&str; 3] = ["USD", "EUR", "GBP"];

/// Line-item descriptions. The seeder appends the line number, so every item
/// row has a distinct description within its claim.
const ITEM_DESCRIPTIONS: [&str; 8] = [
    "Return rail fare",
    "Airport transfer",
    "Overnight accommodation",
    "Working lunch",
    "Conference registration",
    "Taxi to the office",
    "Printed materials",
    "Parking",
];

/// Fixed decimal strings bound to the `NUMERIC(19,4)` amount column and cast
/// by PostgreSQL. They are strings rather than numbers so that no money value
/// ever passes through a Rust float or through Rust arithmetic (ADR-0007).
const ITEM_AMOUNTS: [&str; 6] = [
    "12.5000", "48.0000", "7.2500", "125.9999", "3.1000", "62.4000",
];

/// The line-item categories, cycling. `None` exercises the optional column:
/// `REIMB-R01` makes the category optional and the check allows `NULL`.
const ITEM_CATEGORIES: [Option<&str>; 5] = [
    Some("travel"),
    Some("meals"),
    None,
    Some("lodging"),
    Some("supplies"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Variant;

    fn arguments<const N: usize>(values: [&str; N]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn the_profile_table_matches_the_contract() -> Result<(), String> {
        let table = [
            (Profile::Minimal, 5_usize, 1_usize, 3_usize),
            (Profile::Dev, 200, 8, 60),
            (Profile::Demo, 500, 12, 120),
        ];

        for (profile, notes, projects, tasks) in table {
            if profile.notes() != notes {
                return Err(format!(
                    "profile {}: notes {} != {notes}",
                    profile.as_str(),
                    profile.notes()
                ));
            }
            if profile.projects() != projects {
                return Err(format!(
                    "profile {}: projects {} != {projects}",
                    profile.as_str(),
                    profile.projects()
                ));
            }
            if profile.tasks() != tasks {
                return Err(format!(
                    "profile {}: tasks {} != {tasks}",
                    profile.as_str(),
                    profile.tasks()
                ));
            }
        }

        // C-R1 §SEEDER fixes the claims and the derived items. The item counts
        // are the sum of `1 + (i % 3)` over the claims, less the first claim's
        // single item: minimal 0, dev 49 - 1 = 48, demo 99 - 1 = 98.
        let claims = [
            (Profile::Minimal, 1_usize, 0_usize),
            (Profile::Dev, 25, 48),
            (Profile::Demo, 50, 98),
        ];

        for (profile, claim_count, item_count) in claims {
            if profile.claims() != claim_count {
                return Err(format!(
                    "profile {}: claims {} != {claim_count}",
                    profile.as_str(),
                    profile.claims()
                ));
            }
            if profile.items() != item_count {
                return Err(format!(
                    "profile {}: items {} != {item_count}",
                    profile.as_str(),
                    profile.items()
                ));
            }
        }

        // The arithmetic the insert loop performs, restated row by row: the
        // first claim is the draft with no items and every other claim carries
        // between one and three.
        if items_per_claim(0) != 0
            || items_per_claim(1) != 2
            || items_per_claim(2) != 3
            || items_per_claim(3) != 1
        {
            return Err("the per-claim item rule does not match seeding.md §2.1".to_string());
        }

        if Profile::default() != Profile::Dev {
            return Err("the default profile is not dev".to_string());
        }
        if SEEDED_USERS.len() != 2 {
            return Err("the seeder must always create exactly two accounts".to_string());
        }

        Ok(())
    }

    #[test]
    fn the_id_derivation_is_stable_and_version_four() -> Result<(), String> {
        let first = seeded_id("notes", 17);
        let second = seeded_id("notes", 17);
        if first != second {
            return Err("two calls with the same input produced different ids".to_string());
        }

        if seeded_id("notes", 16) == first || seeded_id("tasks", 17) == first {
            return Err("different inputs produced the same id".to_string());
        }

        if first.get_version_num() != 4 {
            return Err(format!(
                "the derived id is version {}, not 4",
                first.get_version_num()
            ));
        }
        if first.get_variant() != Variant::RFC4122 {
            return Err(format!(
                "the derived id has variant {:?}, not RFC4122",
                first.get_variant()
            ));
        }

        Ok(())
    }

    #[test]
    fn the_argument_parser_accepts_the_documented_flags() -> Result<(), String> {
        let parsed = SeedArgs::parse(arguments([
            "--profile",
            "demo",
            "--reset",
            "--allow-non-empty",
        ]))
        .map_err(|error| format!("the parser rejected valid arguments: {error}"))?;

        if parsed.profile != Profile::Demo {
            return Err("--profile demo did not select the demo profile".to_string());
        }
        if !parsed.reset {
            return Err("--reset was not recorded".to_string());
        }
        if !parsed.allow_non_empty {
            return Err("--allow-non-empty was not recorded".to_string());
        }
        if parsed.help {
            return Err("--help was recorded without being passed".to_string());
        }

        let equals = SeedArgs::parse(arguments(["--profile=minimal"]))
            .map_err(|error| format!("the parser rejected --profile=minimal: {error}"))?;
        if equals.profile != Profile::Minimal {
            return Err("--profile=minimal did not select the minimal profile".to_string());
        }

        let default = SeedArgs::parse(Vec::new())
            .map_err(|error| format!("the parser rejected an empty command line: {error}"))?;
        if default.profile != Profile::Dev {
            return Err("the default profile is not dev".to_string());
        }

        Ok(())
    }

    #[test]
    fn the_argument_parser_rejects_an_unknown_flag() -> Result<(), String> {
        match SeedArgs::parse(arguments(["--bogus"])) {
            Err(message) if message.contains("--bogus") => Ok(()),
            Err(message) => Err(format!("the error does not name the flag: {message}")),
            Ok(_) => Err("an unknown flag was accepted".to_string()),
        }
    }

    #[test]
    fn the_argument_parser_requires_a_profile_value() -> Result<(), String> {
        match SeedArgs::parse(arguments(["--profile"])) {
            Err(message) if message.contains("--profile") => Ok(()),
            Err(message) => Err(format!("the error does not name the flag: {message}")),
            Ok(_) => Err("--profile without a value was accepted".to_string()),
        }
    }

    #[test]
    fn the_help_flag_is_recognised() -> Result<(), String> {
        let parsed = SeedArgs::parse(arguments(["--help"]))
            .map_err(|error| format!("the parser rejected --help: {error}"))?;
        if !parsed.help {
            return Err("--help was not recorded".to_string());
        }
        Ok(())
    }
}
