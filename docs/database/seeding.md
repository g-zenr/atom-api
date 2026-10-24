# The database seeder

> **Status:** Implemented
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `database` agent

`atom-seed` is the development seeder: one command that fills a disposable
database with a complete, deterministic dataset, so the API and the future
frontend can be exercised without hand-crafted rows.

This document is the specification the seeder is built against. It describes
the command, the profiles, the safety guards, determinism, idempotency, the
reset, the post-insert verification, the credentials and the tests.

**The binary, the tests and this specification all exist.** `[[bin]] name =
"atom-seed"` is declared in [`../../Cargo.toml`](../../Cargo.toml), the source
is [`../../src/bin/atom-seed.rs`](../../src/bin/atom-seed.rs), and
[`../../tests/seeder.rs`](../../tests/seeder.rs) drives it with 18 tests. The
status is `Implemented`, not `Verified`: rows 10 and 11 of the `D2.S1`
checkpoint — `code-reviewer` on the seeder diff and `quality-gate` on the
evidence — have not been closed (the table in
[`../engineering/runs/chain/ledger.md`](../engineering/runs/chain/ledger.md)
names them).

The seeder is **development tooling, not product behaviour**. It is not a
requirement in [`../project-truth.md`](../project-truth.md), and
[`../roadmap.md`](../roadmap.md) lists it as `Proposed` until the owner adopts
it. `Proposed` describes adoption of the tool, not the state of the work: the
binary is built and covered by its own tests, and this specification is what
the chains implemented.

> **The seeder is development-only.** It writes credentials that are published
> in this document, and `--reset` deletes **every row owned by the two seeded
> accounts** (A-C-S1-9). It must never be pointed at a production database or at
> a shared database that other people rely on. The guards in §3 exist to catch a
> mistake, not to make the tool safe to aim at production.

---

## 1. The command

```bash
cargo run --bin atom-seed -- --profile dev --reset
```

That is the whole interface. The `--` separates Cargo's arguments from the
binary's; without it, `cargo run` consumes `--profile` itself.

| Argument | Meaning | Default |
| -------- | ------- | ------- |
| `--profile <minimal\|dev\|demo>` | The size of the dataset. | `dev` |
| `--reset` | Delete every row owned by the two seeded accounts, then insert. See §6. | off |
| `--allow-non-empty` | Admit a database that holds accounts outside the seeded set. See §3.3. | off |
| `--help` | Print usage and exit `0`. | — |

- The parser is hand-written over `std::env::args_os`, because the contract
  forbids a new dependency (`C-S1` in
  [`../engineering/runs/chain/ledger.md`](../engineering/runs/chain/ledger.md)).
  There is no `clap`.
- `--profile dev` and `--profile=dev` are both accepted. An unknown flag, a
  missing `--profile` value and an unknown profile name are usage errors: a
  message on stderr and a non-zero exit. No database is contacted.
- Exit status: `0` on success, non-zero for a usage error, a guard refusal
  (§3) or a failed verification (§7). The exact non-zero code is not part of
  the contract; the tests assert failure and the message, not the number.
- The command is non-interactive. It never prompts, and the only destructive
  operation (`--reset`) is an explicit flag.
- `--help` states the blast radius in one sentence: "Deletes every row owned by
  the two seeded accounts, including rows you created through the API while
  logged in as them." The sentence is part of the help text, so it is visible
  before the flag is used, not only after (A-C-S1-9).

### 1.1 What it reads

| Source | Use |
| ------ | --- |
| `.env` | Loaded with `dotenvy::dotenv().ok()`, exactly as [`../../src/main.rs`](../../src/main.rs) does. A missing file is not an error, and a variable already present in the process environment wins. |
| `DATABASE_URL` | Required after `.env` is loaded. It is parsed for the database-name guard with `PgConnectOptions::from_str(...).get_database()` (§3.2) and then used to connect. Its value is never printed. |
| `APP_ENV` | Optional. Unset is allowed; otherwise it is normalised and checked against the allow-list of `G1` (§3.1), which mirrors `Environment::parse` in [`../../src/config/mod.rs`](../../src/config/mod.rs). |

It reads **nothing else**. In particular it does not need `APP_JWT_SECRET`,
`TEST_DATABASE_URL` or any file in `config/`: the password hash uses constants
(§8), and the seeder writes rows directly rather than through the HTTP API.

### 1.2 The run, step by step

1. Parse the arguments. A usage error stops here.
2. `G1`, the environment allow-list (A-C-S1-5). It needs no database.
3. `G2`, the database-name allow-list (A-C-S1-6). It reads the name through
   `PgConnectOptions::from_str(...).get_database()` and opens nothing.
4. Connect.
5. Assert the connected name: `SELECT current_database()` must equal the name
   `G2` approved, and a mismatch is a refusal (A-C-S1-6).
6. `G3` preflight: `SELECT to_regclass('users') IS NOT NULL`. When the table
   exists, run the `G3` count and refuse here, **before** any migration; on a
   fresh database the preflight is skipped (A-C-S1-7).
7. Apply the embedded migrations with `sqlx::migrate!("./migrations")` — the
   same call `src/main.rs` makes. On a fresh database this creates the schema;
   on an up-to-date one it is a no-op. The seeder adds no migration file and
   never edits one.
8. Open one transaction.
9. `G3` again, inside that transaction: the authoritative count (A-C-S1-7).
10. If `--reset` is set, print the blast-radius line and delete the rows in the
    order in §6.
11. Insert each entity in foreign-key order: `users`, `projects`, `tasks` and
    `notes`; then `expense_claims`, `expense_items`, `companies`,
    `job_applications`, `application_events` and `embeddings`, **each only when
    its table exists — see §2**.
12. Recompute `total_amount` for the claims this run inserted, from their items,
    in SQL (§7.1).
13. Run the post-insert verification (§7).
14. `COMMIT`, then print the report.

Steps 8 to 13 run inside one transaction on purpose: `--reset` must never leave
a half-deleted dataset, and a verification failure must not leave a dataset that
the seeder itself would reject. A **refused** run stops at step 2, 3, 5 or 6 —
before step 7 — so it leaves the schema and the data exactly as they were and
sends the target no DDL at all; a role with only `CONNECT` and `SELECT` can be
refused safely (A-C-S1-7). A failed verification rolls back. A migration that
fails part-way is the only case that can leave the schema changed, which is also
true of the API's own start-up.

---

## 2. The profiles

Three profiles exist. The counts are **totals across both seeded accounts**,
not per account.

| Profile | Users | Notes | Projects | Tasks | Claims | Applications |
| ------- | ----: | ----: | -------: | ----: | -----: | -----------: |
| `minimal` | 2 | 5 | 1 | 3 | 1 | 1 |
| `dev` (default) | 2 | 200 | 8 | 60 | 25 | 20 |
| `demo` | 2 | 500 | 12 | 120 | 50 | 40 |

- **Two users, always.** The second account exists so that ownership isolation
  can be demonstrated and tested. The frontend logs in as the first.
- **Ownership split.** For notes, projects, claims, companies and applications,
  row `i` belongs to seeded user `i % 2`. Every profile therefore gives both
  accounts data, and every ownership test has a row on each side. A task
  inherits the owner of its project; an item inherits the owner of its claim;
  an event inherits the owner of its application.
- **Which chain seeds what.** The `S1` chain seeds the four Phase 1 entities:
  `users`, `notes`, `projects`, `tasks`; `R1` extends the seeder with
  `expense_claims` and `expense_items`; `J1`-`J5` extend it with `companies`,
  `job_applications` and `application_events`. `S4` (embeddings, behind
  `--with-embeddings`) and `S5` (the CI smoke run) remain.
- **The seeder must not name a table that does not exist.** `migrations/` ends
  at `0005_job_applications.sql`, so `companies`, `job_applications` and
  `application_events` exist and are seeded; `embeddings` is not there yet, so
  an `INSERT` into it would fail to prepare. The seeder grows by one entity per
  chain, in the same change that adds the table.
- **Coverage.** `dev` and `demo` contain enough claims to cover all five claim
  statuses and enough applications to cover all seven stages. `minimal` seeds
  one of each, so full coverage there is arithmetically impossible; its single
  claim is a `draft` and its single application is `wishlist`, and it proves
  the plumbing rather than the breadth.
- **Derived entities are not profile counts.** `expense_items`,
  `application_events` and `companies` are derived from their parents, and
  `embeddings` is bounded by the ML settings. The chain that added each of the
  first three extended this document in the same change, with the rule it uses;
  the chain that adds `embeddings` must do the same.

The claim and application parts of this specification inherit the plan's
assumptions **A1** (one currency per claim), **A3** (applications and events,
with no generated text), **A5** (single owner, no sharing) and **A6** (money is
`NUMERIC(19,4)`). They are **provisional defaults, owner decision O1**, exactly
as [`../project-truth.md`](../project-truth.md) §18.4 records them. A revision
changes the feature documents and this specification in the same change.

### 2.1 The shape of the data

The fixture text is a fixed corpus of constants in the binary: project names,
note titles and bodies, task titles, claim titles and company names. There is
no fixture file, no network call and no generated text.

| Entity | Shape of the seeded rows |
| ------ | ------------------------ |
| `projects` | Names from a list of twelve, so `demo` can fill all twelve. Descriptions are one short paragraph. |
| `notes` | Searchable technical text: each body contains identifiers, versions and a few sentences a person would actually write. `is_archived` is true for every seventh note (`i % 7 == 6`), so the archive filter has data. |
| `tasks` | Title from a fixed list, `status` cycling `open`, `in_progress`, `done` by `i % 3`, `priority` cycling `low`, `medium`, `high`, `urgent` by `i % 4`. Task `i` belongs to project `i % project_count`, and `position` is the integer `i / project_count`, so the manual order inside a project is stable. A `done` task carries `completed_at`; every other status leaves it `NULL`, because `tasks_completion_consistency_chk` requires exactly that. |
| `expense_claims` | `status` cycling the five values by `i % 5`. A `draft` claim may have no items; a submitted, approved, rejected or paid claim gets at least one. `total_amount` is computed from the items. Claim `i` links to its owner's project `i % project_count` whenever the profile has projects, and has `project_id` `NULL` when it has none (`minimal` has one project, so every claim there is linked). The `NULL` case is covered by `tests/claims_schema.rs`, not by seed data. |
| `expense_items` | `1 + (i % 3)` items per claim, except the first `draft` claim in each profile, which has none. |
| `companies` and `job_applications` | `status` cycling the seven stages by `i % 7`. Each owner's applications are grouped in threes under that owner's companies: within owner `u`'s applications, company `k` employs positions `3k`, `3k + 1` and `3k + 2` (the last group may be short), and two further companies of owner `u` have no application. Every application's company therefore has the application's owner. An application whose `i % 5 == 4` links its `next_action_task_id` to a task owned by the same account; the rest are `NULL`. |
| `application_events` | `1 + (i % 4)` events per application, kinds cycling the six values. |

---

## 3. Safety guards

Four guards, each with its refusal behaviour. Together they are the reason the
command is allowed to exist at all.

### 3.1 `G1` — the environment guard

**Rule (A-C-S1-5).** Unset `APP_ENV` is allowed. Otherwise, after the same
`trim().to_ascii_lowercase()` the configuration loader applies, allow **only**
`development`, `dev` and `local`. **Every other value is refused** — including
`prod`, `staging`, the empty string and a typo.

**Why the list is copied rather than linked.** The accepted set is exactly what
`Environment::parse` in [`../../src/config/mod.rs`](../../src/config/mod.rs)
maps to `Development`; every other value is refused, whether the loader would
map it to `Production` or reject it. The seeder does not link the library crate
to call the parser: a development binary would then pull the vendored Swagger UI
bundle (the `docs` feature) into the build for one match statement. The list is
copied instead, and the only drift mode is a refusal: if the loader ever gains a
new alias, this guard refuses it until the list is updated; it never allows a
value the loader treats as production. That is stricter than the superseded
substring rule, which let `prod` through.

**Refusal.** Nothing is read from the database, nothing is written, the process
exits non-zero and prints, on stderr:

```text
atom-seed: refused: APP_ENV is "prod", which is not an allowed development
environment. Allowed values are "development", "dev" and "local", or unset.
The seeder writes known credentials and deletes rows. Nothing was written.
```

The value is rendered with `{:?}`, never `{}`, so an empty string prints as
`""` and a newline is escaped rather than breaking the message.

### 3.2 `G2` — the database-name guard

**Rule (A-C-S1-6).** Allow a database name only when it is one of

```text
atom_dev  atom_test  atom_local  atom_demo
```

or ends in `_dev`, `_test` or `_local`. Read the name through the driver's own
parser:

```rust
PgConnectOptions::from_str(&url)?.get_database()
```

A parse error, or no database name at all, is a refusal: the guard fails closed.
After connecting, the seeder runs `SELECT current_database()` and refuses unless
it equals the name it approved. "The name in the URL" and "the database actually
connected to" must be the same string.

**Why the driver's parser, not a split.** The integration harness in
[`../../tests/common/mod.rs`](../../tests/common/mod.rs) has its own
`database_name` helper, and it is wrong for this purpose: it does
`split('/').nth(1)`, which takes the **first** path segment, not the last. (An
earlier revision of this document claimed it took the last segment; that was
wrong.) It works for the harness's exact URL shape and breaks on a password that
contains `/`, a Unix-socket path, or a query string. A hand-rolled split is
exactly the bug this guard cannot afford, so the name comes from the same parser
that will build the connection.

**Refusal.** The name check opens nothing. The process exits non-zero and
prints:

```text
atom-seed: refused: the database name "atom" is not allowed. Allowed names are
"atom_dev", "atom_test", "atom_local" and "atom_demo", or a name ending in
"_dev", "_test" or "_local". The name is shown as a Rust string literal and
truncated to 64 characters. Nothing was written.
```

The name is rendered with `{:?}`, never `{}`, and truncated to **64 characters
on a character boundary** before it is rendered; an unbounded name must not be
able to flood the terminal. The message contains the database name and nothing
else from the URL — no host, user, password, query string or full URL. A
connected-name mismatch uses the same rendering:

```text
atom-seed: refused: the connection reports the database "other_db", but the
approved name was "atom_dev". Nothing was written.
```

**The honest limit.** No client-side guard distinguishes a development database
from a production clone restored under an allowed name: `atom_dev` restored from
a production backup passes every check above. The suffix rule is permissive by
design and admits `production_test`; it is a shape check, not a provenance
check. The allow-list is a typo-catcher, not a containment boundary; §8 says
what that means for the credentials.

**The development setup.** `docker-compose.yml` creates `atom`
(`POSTGRES_DB: atom`), which is **not** on the allow-list; the intended
development name is `atom_dev`, which is.
`scripts/create-test-database.sh` creates `atom_dev` next to `atom_test` on a
fresh volume, and `.env.example` points `DATABASE_URL` at `atom_dev`. On an
existing volume, where the init script has already run, create it once:

```bash
docker compose exec postgres createdb -U atom atom_dev
DATABASE_URL=postgres://atom:atom@127.0.0.1:5432/atom_dev \
  cargo run --bin atom-seed -- --profile dev --reset
```

```powershell
docker compose exec postgres createdb -U atom atom_dev
$env:DATABASE_URL = "postgres://atom:atom@127.0.0.1:5432/atom_dev"
cargo run --bin atom-seed -- --profile dev --reset
```

`createdb` is no longer a way around the guard — `atom_dev` is on the list. It
is only how the database is created when the init script did not run.

The gap was closed by making the setup provide a name the guard accepts —
`.env.example` (line 67) points `DATABASE_URL` at `atom_dev` and
`scripts/create-test-database.sh` creates it — not by weakening `A-C-S1-6`,
which still excludes `atom`; `KI-020` is closed on that basis.

### 3.3 `G3` — the foreign-account guard

**Rule (A-C-S1-7).** Refuse when `users` holds any id outside the seeded set,
unless `--allow-non-empty` is passed. The seeded set is exactly two ids: the
deterministic ids of `users/0` and `users/1` (§4.1).

```sql
SELECT COUNT(*) FROM users WHERE id <> ALL($1::uuid[]);
```

It runs twice:

- **Preflight, before the migration step.** The seeder reads
  `SELECT to_regclass('users') IS NOT NULL`. When the table exists, it runs the
  count and refuses before `sqlx::migrate!`; when it does not (a fresh
  database), the preflight is skipped. This is what makes a refusal leave the
  schema untouched, and it removes the need for DDL rights on a refused target.
- **Authoritative, inside the seeding transaction.** After the migrations and
  after the transaction opens, the count runs again. A row that appeared between
  the preflight and the transaction is caught here.

**Refusal.** The count is reported, nothing is inserted or deleted, the process
exits non-zero, and in the authoritative phase the transaction rolls back:

```text
atom-seed: refused: the database holds 3 accounts outside the seeded set.
Pass --allow-non-empty to admit it. --reset deletes every row owned by the two
seeded accounts and leaves the other accounts alone. No data rows were written.
```

**The flag is not a widening of the reset.** `--allow-non-empty` admits the
database; it does not make `--reset` delete rows owned by other accounts. It
also does not protect rows owned by the two seeded accounts — `--reset` deletes
those whoever created them (§6, A-C-S1-9). An account outside the seeded set,
with its notes, projects and tasks, survives every run.

**Why the guard matters.** It is what makes `ON CONFLICT (id) DO NOTHING`
sufficient (§5): if every account in the database is one of the two seeded
accounts, an existing row at a seeded id is a row the seeder wrote. A partial
dataset from a deleted row is the remaining edge case, and it surfaces as a
foreign-key error rather than as data loss.

### 3.4 `G4` — the secret guard

**Rule (A-C-S1-8).** The seeder never prints the value of `DATABASE_URL`, a
password, or a password hash — not on success, not on failure, not in a panic
message. Naming the variable in `--help` is allowed; printing its value is not.

The rule is a classification of every error, not a list of remembered
redactions. The model is `describe_database_error` in
[`../../src/main.rs`](../../src/main.rs) (lines 146–160):

| Error | What may be printed |
| ----- | ------------------- |
| `sqlx::Error::Database` | `kind()` and `code()` only, e.g. `UniqueViolation (SQLSTATE 23505)`. Never `message()`, `detail()`, `constraint()`, `Display` or `Debug`: the message can quote the row and the detail can quote the value. |
| `sqlx::Error::Io(inner)` | A fixed sentence plus the inner message: `the connection could not be established: {inner}`. |
| Every other `sqlx::Error` variant | A fixed sentence. `Configuration` in particular carries the connection string through `Display` and `Debug`, so neither is used. |
| Every `sqlx::migrate::MigrateError` variant | A fixed sentence. The model's `Execute` arm delegates to `describe_database_error` and its final arm calls `other.to_string()`; the seeder copies neither, because a fixed sentence is sufficient and is stricter. |

**Do not copy** the `other => other.to_string()` arm of `describe_migration_error`
in [`../../src/main.rs`](../../src/main.rs): it is the one arm of the model that
repeats a driver message, and a fixed sentence is enough for a migration
failure.

The rest is structural:

- The connection string is held in one local variable and is never formatted.
  `SeedArgs` holds the profile and the two flags only, so it cannot leak the
  URL through a derived `Debug`.
- Refusals and counts are printed with `println!`/`eprintln!` and fixed
  strings. The seeder installs no tracing subscriber and no custom panic hook,
  and it puts neither the URL nor the hash into a panic payload.
- The binary carries `#![deny(clippy::unwrap_used, clippy::expect_used)]` at
  the crate root. There is no `unwrap`, `expect` or `assert` on any value
  derived from `DATABASE_URL` or from a row: a panic message is a print, and it
  is not exempt.
- The success report names entities and integers. It never prints a row.

Two tests assert the classification directly (§9):
`a_connect_failure_prints_no_part_of_the_url` and
`a_constraint_violation_is_reported_by_sqlstate_and_constraint_only`.

**A second binary under SD-5.** [`../security/secure-development.md`](../security/secure-development.md)
SD-5 names `src/bin/atom-docs.rs` as the only development binary allowed
`expect`. `src/bin/atom-seed.rs` is a second binary with a worse input — a
connection string and row data — so the exception in that policy must be
extended to name it, with the `#![deny(...)]` guard above as the enforcement.
The policy file is not this document's to edit; the orchestrator must reconcile
the two. Until it does, the repository's SD-5 check lists the new binary's hits
and the reviewer has no documented exception to cite; that is a documentation
defect, not a code defect.

---

## 4. Determinism

### 4.1 Identifiers

Every id is derived, never generated:

```text
digest = SHA-256("atom-seed/v1/<entity>/<index>")
bytes  = digest[0..16]
bytes[6] = (bytes[6] & 0x0f) | 0x40   // version 4
bytes[8] = (bytes[8] & 0x3f) | 0x80   // RFC 4122 variant
id     = Uuid::from_bytes(bytes)
```

- `<entity>` is the table name in lower-case snake case: `users`, `notes`,
  `projects`, `tasks`, `expense_claims`, `expense_items`, `companies`,
  `job_applications` and `application_events`; `embeddings` is planned.
- `<index>` is the row's zero-based position within the entity, in decimal
  without padding: `users/0`, `notes/17`, `tasks/103`.
- The version and variant bits are set so the value is a well-formed UUIDv4
  and every consumer — the `uuid` crate, SQLx, `Uuid::parse_str` — accepts it.
  It is still not random: the same input always produces the same id.
- The `v1` in the namespace is part of the contract. Changing the derivation,
  or renaming an entity, produces a different id set: the old rows become
  foreign rows that `G3` refuses and `--reset` does not delete. That is
  deliberate — the answer is a manual cleanup or a fresh database, never an
  automatic delete of rows the seeder no longer owns.
- `sha2` and `uuid` are already dependencies; no new crate is needed.

### 4.2 Timestamps

The base instant is fixed:

```text
2026-01-01T00:00:00Z
```

Every timestamp is the base plus a deterministic offset. There is one constant
table in the source; these are the reference values:

| Entity | `created_at` for row `i` |
| ------ | ------------------------ |
| `users` | base + `i` days |
| `projects` | base + `(7 + i)` days |
| `notes` | base + 1 day + `i × 30` minutes |
| `tasks` | base + 14 days + `i × 20` minutes |
| `expense_claims` | base + 21 days + `i × 6` hours |
| `expense_items` | its claim's `created_at` + `(j + 1)` hours |
| `companies` | base + 20 days + `i × 3` hours |
| `job_applications` | base + 28 days + `i × 4` hours |
| `application_events` | its application's `created_at` + `(j + 1)` days |
| `embeddings` | its source's `created_at` + 1 hour |

The dependent values keep the row internally consistent and never read the
clock:

- `updated_at` is the latest of the row's own state timestamps — `created_at`,
  `submitted_at`, `decided_at` and `paid_at` for a claim, `completed_at` for a
  done task, `created_at` otherwise — plus one hour. It is never earlier than
  a state change it is supposed to describe.
- A `done` task's `completed_at` is `created_at + 2 hours`; every other status
  leaves it `NULL`.
- A claim's `submitted_at` is `created_at + 1 day`; `decided_at` is
  `created_at + 2 days`; `paid_at` is `created_at + 3 days`. Each is set
  exactly when the status requires it, so
  `expense_claims_lifecycle_timestamps_chk` holds.
- An item's `incurred_on` is `(claim.created_at − 3 days)::date`.
- A non-`wishlist` application's `applied_on` is `(created_at − 1 day)::date`;
  a `wishlist` application has no `applied_on`.
- An event's `occurred_at` is `created_at + j` days.

Because the values are fixed, two `--reset` runs produce the same ids and the
same timestamps. Changing a constant table entry is a change to this document
in the same commit.

### 4.3 What is forbidden

- No `rand` for ids, timestamps, ordering or content. No `Uuid::new_v4`.
- No `Utc::now()`, no `SystemTime::now()`, no `NOW()` or `CURRENT_TIMESTAMP` in
  an insert. Every timestamp column is listed in the `INSERT` with an explicit
  value, so the schema's `DEFAULT NOW()` never evaluates.
- No `f32` or `f64` in any computation. `tasks.position` is the only
  `DOUBLE PRECISION` column; its value is an integer produced by integer
  division and bound directly as an `f64` parameter, with no arithmetic done in
  floating point.

There is exactly one non-deterministic value in the run: the salt inside the
Argon2id hash (§8). It is generated inside `argon2` from the OS random source,
it is not used for identity, and the hash column is never part of what the
frontend displays or a test compares. Two runs therefore store different,
equally valid hashes for the same published password. A fixed salt would make
them identical and would be the worse choice.

---

## 5. Idempotency without `--reset`

Every insert carries:

```sql
INSERT INTO notes (id, user_id, title, content, is_archived, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7)
ON CONFLICT (id) DO NOTHING;
```

- `EXISTS` is never used as a pre-check. The database decides, so two
  invocations cannot both insert the same id.
- `rows_affected` is the number of rows actually inserted; the rest are
  counted as skipped. The run reports both, per entity and in total:

  ```text
  atom-seed: profile dev
    users      inserted 2     skipped 0
    notes      inserted 200   skipped 0
    projects   inserted 8     skipped 0
    tasks      inserted 60    skipped 0
    claims     inserted 25    skipped 0
    items      inserted 48    skipped 0
  atom-seed: verification passed (6 checks); 343 rows inserted, 0 skipped
  atom-seed: accounts owner@atom.local and other@atom.local
  ```

- A second run on the same database reports `inserted 0` and every row as
  skipped. The seeder **never updates** an existing row: a seeded row whose
  content has drifted is left alone, because silently rewriting a row a
  developer has edited is worse than saying "skipped".
- The seeder assumes an all-or-nothing prior state. If a project was deleted
  by hand, its tasks went with it (the `0003` cascade) and re-inserting the
  tasks fails on the foreign key. The remedy is `--reset`, which deletes every
  row owned by the two seeded accounts before inserting (§6, A-C-S1-9); the
  error names the constraint, not the row's values.

---

## 6. `--reset`

`--reset` deletes **every row owned by the two seeded accounts** and then
inserts (A-C-S1-9). "Owned" is the `user_id` predicate, not "created by the
seeder": notes, projects, tasks, claims and applications a developer created
through the API while logged in as `owner@atom.local` or `other@atom.local` are
deleted too. It runs inside the same transaction as the inserts, so an
interruption leaves the previous dataset intact.

Before the first delete it prints the scope, so the blast radius is visible in
the log and not only in `--help`:

```text
atom-seed: reset: deleting every row owned by the two seeded accounts
(owner@atom.local and other@atom.local), including rows created through the
API while logged in as them
```

The delete order is the foreign-key order. The run executes a table's
statement only when the table exists: today every row below is executed except
the one for `embeddings`, which is the contract for the chain that adds the
table.

| # | Table | Predicate | Exists today |
| - | ----- | --------- | ------------ |
| 1 | `embeddings` | `user_id = ANY($1)` | no — `0006` |
| 2 | `application_events` | `user_id = ANY($1)` | yes — `0005` |
| 3 | `expense_items` | `user_id = ANY($1)` | yes — `0004` |
| 4 | `expense_claims` | `user_id = ANY($1)` | yes — `0004` |
| 5 | `job_applications` | `user_id = ANY($1)` | yes — `0005` |
| 6 | `companies` | `user_id = ANY($1)` | yes — `0005` |
| 7 | `refresh_sessions` | `user_id = ANY($1)` | yes |
| 8 | `notes` | `user_id = ANY($1)` | yes |
| 9 | `tasks` | `project_id IN (SELECT id FROM projects WHERE user_id = ANY($1))` | yes |
| 10 | `projects` | `user_id = ANY($1)` | yes |
| 11 | `users` | `id = ANY($1)` | yes |

- `$1` is the two seeded user ids, bound as a `uuid[]`. There is one statement
  per table, and no statement builds SQL from strings.
- **`tasks` has no `user_id`** (§6 of [`schema.md`](schema.md)). Its predicate
  goes through `projects`, which is also why the order deletes tasks before
  projects.
- `refresh_sessions` is deleted explicitly even though the `users` cascade
  would remove it. The order is the contract, and an explicit statement does
  not depend on a cascade staying in place.
- Applications are deleted before companies, because
  `job_applications.company_id` uses `ON DELETE NO ACTION` and a company that
  still has applications must not be deletable (§11.4 of
  [`schema.md`](schema.md)).
- Claims are deleted before projects. The `project_id` foreign key is
  `ON DELETE SET NULL`, so either order is valid; deleting the child first
  avoids a pointless update.
- Two foreign keys reach rows the seeder does not own. Deleting a seeded
  session `SET NULL`s `refresh_sessions.replaced_by_session_id` on a foreign
  session that pointed at it, and from `0004` deleting a seeded project
  `SET NULL`s a foreign claim's `project_id`. Both are `ON DELETE SET NULL`
  acting as designed: the seeded row is the parent, so the child survives with a
  cleared link, and the reset predicate never touched it.

**What it never does.**

- It never runs `TRUNCATE`, on any table, for any profile.
- It never runs `DELETE FROM users` without the id predicate, and it never
  deletes a row owned by an account outside the seeded set — not even with
  `--allow-non-empty`. A foreign row can lose a link to a seeded parent through
  `ON DELETE SET NULL`; that is the side effect above, not a delete.
- `--reset` never touches `_sqlx_migrations`. The migration step in §1.2 does,
  because recording an applied migration is what that table is for; the reset
  neither deletes a row there nor rolls a migration back.
- It never names a table that is not in the list above.
- It does not delete the database, the schema or the role.

**Side effect on a running API.** Deleting the seeded users deletes their
refresh sessions, so a browser or terminal already logged in as
`owner@atom.local` receives `401` on its next request and must log in again.
Anything that account wrote through the API is deleted with it. That is
expected, and it is why the command prints the scope before it starts.

---

## 7. Post-insert verification

After the inserts and before `COMMIT`, the seeder checks its own output. A
failed check rolls the transaction back and exits non-zero; the message names
the check, never the connection string.

The checks below cover the Phase 1 entities from `S1` and the later entities as
their chains add them. A check whose table does not exist yet is not run, for
the same reason the seeder does not insert into it (§2).

### 7.1 `V1` — a claim's total is the sum of its items

For every seeded claim, in SQL:

```sql
SELECT COUNT(*)
FROM expense_claims c
WHERE c.id = ANY($1::uuid[])
  AND c.total_amount <> COALESCE(
      (SELECT SUM(i.amount) FROM expense_items i
       WHERE i.claim_id = c.id AND i.user_id = c.user_id),
      0
  );
```

The expected count is `0`. A claim with no items must carry `0.0000`
(`REIMB-R01`, `REIMB-R03`), which the `COALESCE` covers.

The seeder never adds money in Rust. After the items are inserted, one
statement sets each total from the sum of that claim's items — a correlated
`UPDATE ... SET total_amount = (SELECT SUM(...))` — restricted to the claims
this run inserted, and `V1` re-reads every seeded claim. This keeps every
money value exact and needs neither a decimal crate nor a `f64`. A skipped claim
keeps its stored total: if it no longer matches its items, `V1` fails and
`--reset` is the remedy, which is what keeps the "never updates an existing row"
rule in §5 true.

### 7.2 `V2` — every row belongs to one owner

| Check | Query shape |
| ----- | ----------- |
| A task's project is owned by the seeded set | `tasks JOIN projects ON projects.id = tasks.project_id WHERE projects.user_id <> ALL($1)` |
| A claim's project shares the claim's owner | `expense_claims LEFT JOIN projects ... WHERE project_id IS NOT NULL AND projects.user_id <> expense_claims.user_id` |
| An application's company shares the application's owner | `job_applications JOIN companies ... WHERE companies.user_id <> job_applications.user_id` |
| An item shares its claim's owner | `expense_items JOIN expense_claims ... WHERE expense_items.user_id <> expense_claims.user_id` |
| An event shares its application's owner | `application_events JOIN job_applications ... WHERE application_events.user_id <> job_applications.user_id` |
| No row names a user outside the seeded set | the `user_id` of every seeded table is in `$1` |

Every expected count is `0`. Checks for a table that does not exist yet are
added by the chain that creates it.

### 7.3 `V3` — task completion agrees with the status

`V3` runs exactly one query, over the tasks this run seeded:

```sql
SELECT COUNT(*) FROM tasks
WHERE id = ANY($1) AND ((status = 'done') <> (completed_at IS NOT NULL));
```

The expected count is `0`. The `0003` CHECK
(`tasks_completion_consistency_chk`) already makes the opposite
unrepresentable; `V3` proves the seeder's own distribution obeys it, and
`every_seeded_task_has_a_consistent_completion_timestamp` in
[`../../tests/seeder.rs`](../../tests/seeder.rs) asserts the same rule (`G8`).

`V3` checks nothing else. It does not read `expense_claims`, and it does not
assert that any status appears at all. The claim lifecycle is enforced when
each row is written by `expense_claims_lifecycle_timestamps_chk` in
[`../../migrations/0004_expense_claims.sql`](../../migrations/0004_expense_claims.sql),
and `a_seeded_claim_total_equals_the_sum_of_its_items` in
[`../../tests/seeder.rs`](../../tests/seeder.rs) asserts that the constraint is
present on the table and that the `dev` profile stores all five statuses, so
every arm of the constraint has data behind it.

### 7.4 `V4` — the counts match the profile, including the derived ones

Every entity the profile fixes is counted **restricted to the seeded set**:
`users` by id, `notes`, `projects`, `tasks` via their projects, and
`expense_claims` and `expense_items` by `user_id`; the remaining owned tables
join this list as their chains add them. The counts must equal the §2 table.

Derived entities are asserted **exactly**, from the same formula that inserts
them (A-C-S1-10), never as a lower bound:

| Derived entity | Expected count |
| -------------- | -------------- |
| `expense_items` | The sum over claim `i` of `1 + (i % 3)`, except the first `draft` claim of the profile, which contributes `0`. |
| `application_events` | The sum over application `i` of `1 + (i % 4)`. |
| `companies` | `Σ over the two owners of (ceil(applications_of_that_owner / 3) + 2)`, from the rule in §2.1. |

A lower bound is unsafe here, and the reason is the deterministic id: a
re-inserted parent gets the **same** id, so a child row that survived on a table
missing from the delete list re-attaches to the new parent instead of becoming
an orphan. The count is then too high, and an "at least one" assertion blesses
it. The exact count fails, which is the point of `--reset`.

Counting the seeded set, not the table, is what makes an `--allow-non-empty`
run meaningful: a row owned by another account does not make the check fail.

### 7.5 `V5` — the documented credential still works

The seeder reads `password_hash` back for `owner@atom.local` and verifies the
documented password against it with the `argon2` crate. The parameters come
from the PHC string, so this works even if `config/auth.toml` changes later.

This is not a repeat of the hash the seeder just computed. On a non-reset run
the row may be older than the binary, and this is the check that notices when
the published credential no longer logs in. A developer who changed the seeded
password through the API sees `V5` fail and uses `--reset`.

### 7.6 `V6` — every user table is named in the seeder's list

The seeder holds a `const` list of the user tables it handles: the tables it
inserts into and the tables `--reset` deletes from, including
`refresh_sessions`, which is deleted but never inserted. At the end of the run
it compares that list with the schema:

```sql
SELECT table_name
FROM information_schema.tables
WHERE table_schema = 'public'
  AND table_type = 'BASE TABLE'
  AND table_name <> '_sqlx_migrations';
```

Every returned name must be in the `const` list; a single unlisted table fails
the run and names the table (A-C-S1-10). A `to_regclass` loop over the list is
an equivalent implementation; the comparison above is the specified one because
it also catches a table the list has never heard of.

**What `V6` guarantees, exactly.** One direction only: every base table in
`public` must be named by `SEEDED_TABLES`. It is a completeness check on the
constant, not on the run, and it does not compare the constant with the
`--reset` statement list or with the insert list. A table added to the constant
and nowhere else therefore still passes: the run would neither insert its rows
nor delete them, and `V6` would say nothing. That is a residual gap in the
check, recorded here rather than papered over; keeping the two lists in step is
a review obligation, not something the seeder verifies.

**Why the check still matters.** A chain that adds a table and forgets the
constant outright fails the run, which is the common omission. With
deterministic ids a stale child on a table missing from the delete order
re-attaches to the re-inserted parent (§7.4), so the defect is silent until
someone reads the data; `V6` turns the "not named at all" case into a failed
run, and the "named but not deleted" case is caught by nothing.

### 7.7 Failure

```text
atom-seed: verification failed: V2 ownership (applications): 1 application
belongs to another owner's company. The transaction was rolled back; the
database is unchanged.
```

The expected and the actual count are printed for a count check. The hash and
the URL never are.

---

## 8. Credentials

| Account | Email | Password | Display name |
| ------- | ----- | -------- | ------------ |
| owner | `owner@atom.local` | `atom-dev-password-1234` | `Owner` |
| other | `other@atom.local` | `atom-dev-password-1234` | `Other` |

- **They are fake and development-only.** `.local` is a reserved
  multicast-DNS name, so the addresses cannot receive mail. The password is
  published in this document and must never be reused anywhere.
- Both rows have `is_active = TRUE`.
- The emails are already trimmed and lower-cased, as `users_email_normalized_chk`
  requires, and they are inside the 3–254 character limit.

### 8.1 The hash

- It is Argon2id, in the PHC format that `users.password_hash` stores and
  `PasswordHasher::verify` accepts.
- It is computed **once** and the same string is inserted for both rows.
  Hashing per user would double the slowest part of the run for no benefit in
  a development fixture. The visible consequence is that the two rows share a
  hash, which is acceptable precisely because the password is public.
- The parameters mirror [`../../config/auth.toml`](../../config/auth.toml):
  memory 19 456 KiB, 2 iterations, parallelism 1. They are constants in the
  seeder, not a read of `config/`, because the seeder must run without
  `APP_JWT_SECRET` and because a PHC string carries its own parameters — a
  stored hash keeps verifying after the configuration changes.
- The hash is never printed, and no test asserts a particular hash value.

### 8.2 The password policy

`atom-dev-password-1234` is 22 characters with lower case, digits and hyphens,
but **no upper-case letter**. The public registration policy in
`src/users/service.rs` requires one, so the API would reject this string at
`POST /auth/register` with `422`.

That does not affect the seeder: it writes the hash directly, bypassing
registration, and login verifies the hash rather than the policy. It does mean
that a test or a frontend form cannot use this string to register a new
account. The two facts are deliberate and are recorded here so nobody "fixes"
the password by adding an upper-case letter without changing this document, the
`S1` test and every demo script that quotes it.

### 8.3 What the accounts are

The two rows are **live credentials**: `is_active = TRUE`, a password published
in this document, and a login that succeeds. Three consequences:

- A database holding them must never be reachable by a non-developer client —
  not a shared staging host, not a tunnel, not a demo laptop on a public
  network. The password is public; anybody who can reach the API can log in.
- The guards are typo-catchers, not a containment boundary (§3.2). A production
  clone restored under `atom_dev` passes `G2`, and `V5` only proves that the
  stored hash matches the published password — it does not prove that the
  account is safe to expose.
- `V5` failing means the row was modified, not that the account is safe: a
  changed hash is still a live account, it is just not the documented one.

**Recommended follow-up, not part of `S1`:** refuse to start in
[`../../src/main.rs`](../../src/main.rs) when the environment is production and
either seeded email exists in `users`. That is an application-level containment
check, and it belongs to the hardening chain, not to the seeder.

---

## 9. Tests

`tests/seeder.rs` exists. Every test drives the real binary through
`env!("CARGO_BIN_EXE_atom-seed")`, because the guards, the argument parser and
the output are properties of the process, not of a library function.

| Test | Proves |
| ---- | ------ |
| `the_seeder_refuses_a_production_environment` | `G1` (A-C-S1-5): `APP_ENV` = `prod`, `staging`, `""`, `Production` in any case and a typo each exit non-zero and name the reason; `development`, `dev`, `local` and unset pass the guard. |
| `the_seeder_refuses_a_database_whose_name_is_not_a_development_database` | `G2` (A-C-S1-6): a name that is neither `atom_dev`/`atom_test`/`atom_local`/`atom_demo` nor ends in `_dev`/`_test`/`_local` exits non-zero, and the refusal renders it as a Rust string literal. The test points at a name that does not exist, proving the name check runs before the connection. |
| `the_seeder_refuses_a_database_that_holds_foreign_rows` | `G3`: one account outside the seeded set refuses the run. |
| `an_override_flag_admits_a_database_that_holds_foreign_rows` | `G3`'s flag: the run succeeds and the foreign account still exists afterwards. |
| `the_dev_profile_inserts_the_documented_counts` | `G9`: the `users`, `notes`, `projects` and `tasks` counts of §2, measured in the database, and that each seeded account owns rows of its own. |
| `a_second_run_skips_every_existing_row` | `G6`: `ON CONFLICT (id) DO NOTHING`, reported as skipped. |
| `a_reset_run_reproduces_the_same_ids_and_timestamps` | `G5`: two `--reset` runs produce equal ids, `created_at` and `updated_at`. |
| `the_seeder_never_prints_the_connection_string_or_a_hash` | `G4`: stdout and stderr contain neither the URL value, the password nor the hash read back from `users`. |
| `a_connect_failure_prints_no_part_of_the_url` | `G4` (A-C-S1-8): with `DATABASE_URL=postgres://atom:atom@127.0.0.1:1/atom_test`, stdout and stderr contain neither `postgres://`, nor `127.0.0.1`, nor the credential portion `:atom@`. The last assertion is written as `:atom@` so the binary's own `atom-seed` prefix is not a false positive. |
| `a_constraint_violation_is_reported_by_sqlstate_and_constraint_only` | `G4` (A-C-S1-8): a deterministic unique violation prints the SQLSTATE `23505` and the `UniqueViolation` kind, and neither the row value nor the driver `DETAIL`. "Constraint only" in the name means the violation kind, not the constraint's name or its row. The test inserts a foreign account with the email `owner@atom.local` under a different id, runs with `--allow-non-empty`, and expects the `users_email_unique_idx` violation when the seeded row is inserted. |
| `seeded_tasks_belong_to_projects_of_the_same_owner` | `G7`: the task/project join of §7.2. |
| `every_seeded_task_has_a_consistent_completion_timestamp` | `G8`: the completion rule of §7.3. |
| `the_help_text_states_the_blast_radius_without_a_url_or_a_hash` | A-C-S1-9 and `G4`: `--help` succeeds with no `DATABASE_URL` and no `APP_ENV`, documents all four flags, states what `--reset` deletes, and prints no URL, password or hash. |
| `an_unknown_flag_is_a_usage_error_that_names_the_flag` | §1: an unknown flag exits non-zero with `atom-seed: usage error:`, names the flag, and is not reported as a guard refusal. |
| `the_seeded_accounts_are_the_published_development_credentials` | §8 and `V5`: exactly the two documented emails, display names and `is_active`; the published password verifies against each stored PHC hash. |
| `a_reset_deletes_rows_owned_by_the_seeded_accounts` | A-C-S1-9: a note written for the seeded owner after the first run is gone after `--reset`, and the documented `minimal` counts are intact. |
| `the_seeder_dev_profile_inserts_the_documented_claim_counts` | §2 and §7.4 for claims: `dev` inserts 25 claims and 48 items, on the derived ids, with claim 0 the only itemless draft. |
| `a_seeded_claim_total_equals_the_sum_of_its_items` | `V1` and §7.3: every seeded claim's `total_amount` equals the sum of its items, compared by PostgreSQL in `numeric`; all 25 are present, `expense_claims_lifecycle_timestamps_chk` exists, and `dev` stores all five statuses. |

Unit tests in `src/bin/atom-seed.rs` cover the three pieces that are pure
functions:

- **The profile table.** Each variant returns the counts of §2; `dev` is the
  default when `--profile` is absent.
- **The id derivation.** Two calls with the same entity and index agree; two
  different indices differ; the bytes are a valid version-4 UUID with the RFC
  4122 variant.
- **The argument parser.** `--reset` sets the flag; `--profile demo` selects
  demo; an unknown flag is an error; `--help` prints usage and exits `0`.

### 9.1 How the tests run

- The tests use the disposable database named by `TEST_DATABASE_URL`. Each
  test sets the child's `DATABASE_URL` to that value, and the harness helper
  `common::test_database_url()` refuses any name that does not contain `test`.
  That helper keeps its own rule; it is not `G2`, and it only chooses the test
  database. The tests never touch the developer's `DATABASE_URL`, and they never
  truncate a table.
- The shared test database is **not** empty: every other integration test
  creates an account and leaves it there. Every seeder invocation that is
  expected to succeed therefore passes `--allow-non-empty`. That flag does not
  widen `--reset`, so rows owned by the other tests' accounts survive.
- `G1` and the name half of `G2` need no database at all, which is why they can
  assert a refusal without creating a database with a forbidden name. The
  `current_database()` half of `G2` runs after connecting; it is defence in
  depth, and no locked test constructs a name-confusion scenario.
- The count assertions query the database. The skipping assertions read the
  per-entity `inserted` and `skipped` numbers from the report, so that line's
  shape is part of the test surface, as is the `atom-seed: refused:` prefix of
  a refusal. Nothing else in the prose output is parsed.
- The seeder tests share one deterministic dataset, so a test that runs while
  another is seeding cannot observe a different one: the inserts collide on the
  same ids, the second statement waits for the first transaction, and
  `ON CONFLICT (id) DO NOTHING` then reports the rows as skipped.
- The `G3` test creates exactly one foreign account of its own before the run,
  rather than depending on whatever the shared database happens to contain. It
  also asserts that `_sqlx_migrations` is unchanged by the refusal. That
  assertion does **not** prove the preflight runs before the migration step:
  the test applies the migrations with `migrated_pool()` before it reads
  `_sqlx_migrations`, so the before-and-after comparison is identical by
  construction. The ordering is implemented in the binary (A-C-S1-7); no test
  distinguishes it, and that is a residual gap rather than a proven property.
- The two `G4` tests (`a_connect_failure_prints_no_part_of_the_url` and
  `a_constraint_violation_is_reported_by_sqlstate_and_constraint_only`) assert
  the absence of substrings and the presence of the SQLSTATE; they parse no
  prose beyond the classification §3.4 specifies.

---

## 10. Refusals, worked

Both examples are real shapes of the message, with no connection string.

**`G1`.** `APP_ENV=prod` — the alias the superseded substring rule let through:

```console
$ APP_ENV=prod cargo run --bin atom-seed -- --profile dev --reset
atom-seed: refused: APP_ENV is "prod", which is not an allowed development
environment. Allowed values are "development", "dev" and "local", or unset.
The seeder writes known credentials and deletes rows. Nothing was written.
$ echo $?
1
```

`APP_ENV=staging`, a typo and `APP_ENV=""` produce the same shape with the
value rendered as a Rust string literal, so the empty string appears as `""`
(A-C-S1-5).

**`G2`.** The repository's default local database name:

```console
$ cargo run --bin atom-seed -- --profile dev --reset
atom-seed: refused: the database name "atom" is not allowed. Allowed names are
"atom_dev", "atom_test", "atom_local" and "atom_demo", or a name ending in
"_dev", "_test" or "_local". The name is shown as a Rust string literal and
truncated to 64 characters. Nothing was written.
$ echo $?
1
```

What neither message contains: the host, the port, the user, the password, the
query string, the full URL, or any hash. The same holds for a `G3` refusal, a
failed verification, a SQLx error and a usage error.

---

## 11. Traceability to the locked contract

The seeder contract `C-S1` is in
[`../engineering/runs/chain/ledger.md`](../engineering/runs/chain/ledger.md),
with the security-review amendments **A-C-S1-5 … A-C-S1-10**. `G1` and `G2` in
the body of `C-S1` are **superseded** by A-C-S1-5 and A-C-S1-6; this document
implements the amended rule.

| Invariant | Subject | Section |
| --------- | ------- | ------- |
| `G1` (A-C-S1-5) | environment allow-list | §3.1 |
| `G2` (A-C-S1-6) | database-name allow-list and connected-name assertion | §3.2 |
| `G3` (A-C-S1-7) | foreign-account guard, preflight and authoritative | §3.3 |
| `G4` (A-C-S1-8) | no URL, password or hash printed; every error classified | §3.4 |
| `G5` | determinism across runs | §4 |
| `G6` | idempotency without `--reset` | §5 |
| `G7` | every seeded row has one owner | §7.2 |
| `G8` | task completion consistency | §7.3 |
| `G9` (A-C-S1-10) | counts match the profile, derived ones exactly | §7.4 |
| A-C-S1-9 | `--reset` blast radius | §1 and §6 |
| A-C-S1-10 | every user table is in the seeded-table list (§7.6 states the residual gap) | §7.6 |

The plan's profiles and the credential policy are in
[`../engineering/runs/chain/plan.md`](../engineering/runs/chain/plan.md) §8.
Where this document is more specific than either — the id algorithm, the
timestamp offsets, the verification queries, the test database's
`--allow-non-empty` requirement — the detail is what makes the contract
implementable, and it does not change the contract.

---

## 12. Operating notes

- **The binary is not a service.** It exits when the run finishes. Nothing
  schedules it, nothing calls it from the API, and it is not part of the
  deployed image's request path.
- **The seeder is not a fixture loader for tests.** The integration suite
  builds the rows it needs through the API or SQL; `tests/seeder.rs` is the
  only test that runs the binary.
- **CI.** The `S5` smoke run executes the seeder against the disposable CI
  database and asserts the `dev` counts. The CI database is named `atom_test`,
  which is on `G2`'s allow-list, so no override is needed.
- **The frontend.** The two accounts are stable across runs, which is the
  point: a screenshot, a saved login and a manual test script keep working
  after a reseed. `--reset` deletes anything written as the seeded accounts and
  invalidates their refresh sessions (§6); it does not change the credentials.
- **A second writer.** The seeder assumes it is the only process writing to
  the database. It takes no advisory lock. Two concurrent runs are not
  supported.
- **The schema.** The seeder only writes tables that exist. Adding a table
  without extending the seeder is an incomplete chain. `V6` (§7.6) fails the
  run while the table is missing from `SEEDED_TABLES`; it does not check the
  inserts or the delete order, so a table named in the constant and nowhere
  else still passes. [`../roadmap.md`](../roadmap.md) states the rule.

---

## 13. Related documents

- [`README.md`](README.md) — the database index and the local setup.
- [`schema.md`](schema.md) — every table, constraint and index the seeder
  writes to; §11 is the proposed Phase 2 schema.
- [`migration-policy.md`](migration-policy.md) — why the seeder adds no
  migration file and why every schema change comes with its model change.
- [`../engineering/runs/chain/plan.md`](../engineering/runs/chain/plan.md) §8 —
  the plan's seeder section.
- [`../engineering/runs/chain/ledger.md`](../engineering/runs/chain/ledger.md) —
  the locked `C-S1` contract and the run's evidence.
- [`../architecture/deployment.md`](../architecture/deployment.md) — the
  development-only rule in the deployment context.
- [`../security/secure-development.md`](../security/secure-development.md) —
  SD-5 and the binary that this specification asks to add to its allow-list.
- [`../../src/main.rs`](../../src/main.rs) — `describe_database_error`, the
  model for §3.4.
- [`../api/conventions.md`](../api/conventions.md) — the money and pagination
  conventions the seeded data is shaped to demonstrate.
