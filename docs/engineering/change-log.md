# ATOM change log

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-29
> **Owner:** maintained by the `docs` agent

The chronological record of substantial changes to the ATOM backend, newest
first. It explains **how** the system reached its current state; it does not
define the target state. The requirements live in
[`../project-truth.md`](../project-truth.md), the current state in
[`implementation-status.md`](implementation-status.md), and the reviews in
[`review-log.md`](review-log.md).

---

## How to read this log

- **Newest first.** The most recent entry is at the top.
- **Append-only.** An entry is never rewritten. An inaccurate statement is
  corrected by adding a clearly marked **Correction** to the same entry, with
  its own date and a reason.
- **One entry per substantial change.** A change is substantial when it alters
  behaviour, a contract, the schema, a security control or the architecture.
  A typo fix does not need an entry.
- **The checks are recorded with their real results.** A check that did not run
  is recorded as *not run*, with the command needed to run it. A result is
  never claimed that was not observed. See
  [`../testing/verification-policy.md`](../testing/verification-policy.md) §4.

## How to add an entry

1. Copy [`../templates/change-record.md`](../templates/change-record.md).
2. Give it the next identifier: `ATOM-CHANGE-YYYYMMDD-NNN`, where `NNN` is a
   three-digit sequence that has not been used before. Identifiers are never
   reused, and the date is the UTC date of the change.
3. Fill in every section. A section that does not apply says
   `Not applicable: <reason>`; it is never deleted, because a deleted section
   hides the question.
4. Insert the entry directly under the `## Entries` heading, above the previous
   entry.
5. Run the three quality gates and record their real output.

## Entries

Entries from before 2026-10-27 are in
[`archive/change-log-2026-phase-1-2.md`](archive/change-log-2026-phase-1-2.md),
moved verbatim by `ATOM-CHANGE-20261029-001` to keep this file small enough
to read whole. They remain part of the record and are never rewritten.

### ATOM-CHANGE-20261029-001 - The knowledge base moves its history to an archive

**Date:** 2026-10-29 (UTC)
**Type:** Documentation - knowledge-base structure
**Author:** the orchestrator, on the owner's decision.
**Status:** Complete - the full gate run (`scripts/gate.ps1`) is **all six at
`0`** on the restructured tree (fmt, clippy, test, nodefault, docs, strict);
`cargo test --all-targets --locked` passed with **536 tests, 0 failed**, which
also covers the four source comments that were repointed. The docs checker
reports `OK (0 warning(s))` in both modes after every step of the move.

#### Why

The knowledge base had grown to ~585K tokens across 93 files, and the run
logs dominated it: the change log (~49K tokens), the Phase 2 run ledger
(~41K) and the review log (~19K) - around 110K tokens of append-only history
for a run that is complete and tagged. Every intake read several of them, the
agent context paid for it on every prompt, and the same status evidence was
restated in up to nine files, which is precisely how the `K1` documentation
figures went stale three times. The owner's direction: keep the governance,
shrink the working set, and do not bloat the context a prompt starts with.

#### What changed

| Area | Change |
| ---- | ------ |
| Change log | Entries from 2026-10-21 to 2026-10-26 (3 049 lines) moved to [`archive/change-log-2026-phase-1-2.md`](archive/change-log-2026-phase-1-2.md), entry text unchanged and link targets repointed for the new location; the hot file keeps the instructions and the entries from 2026-10-27 onward, with a pointer under `## Entries` |
| Review log | Full records from `ATOM-REVIEW-20261021-001` through `-20261026-002` (821 lines) moved to [`archive/review-log-2026-10.md`](archive/review-log-2026-10.md), text unchanged and link targets repointed; the hot file keeps the every-review summary list and the two most recent full entries |
| Known issues | The fourteen **closed** `KI` entries moved to [`archive/known-issues-closed.md`](archive/known-issues-closed.md), text unchanged and link targets repointed; the hot file keeps the format, the open issues, and the summary table, which gains the `KI-022` and `KI-024` rows it was missing |
| Phase 2 run | `docs/engineering/runs/chain/` moved whole (git rename, history preserved) to [`runs/archive/phase-2/`](runs/archive/phase-2/ledger.md); a stub [`runs/chain/ledger.md`](runs/chain/ledger.md) records that no run is active and points at the archive. References from archived records into the run ledger land on that stub and follow its pointer one hop onward — accepted deliberately, so the archived bodies keep exactly one kind of edit (their own link depth) |
| Archive index | [`archive/README.md`](archive/README.md) lists what moved, when, and by which change |
| Intake rule | `AGENTS.md` section 5 is now **tiered**: always read the index, the one feature card, the governing `project-truth` section and the real diff; architecture, ADRs, full KI entries, the configuration reference and the standards are retrieved on demand; documents are passed to subagents as paths, never pasted |
| Single sourcing | `implementation-status.md` section 4 records the rule: a feature's status and evidence are written once, in its feature card; the overview row and the index rows carry the status word and a pointer, never a restatement. The `K1` row was trimmed to that shape |

554 relative links were rewritten across the moved and repointed documents,
each resolved against the original tree rather than guessed. Every moved
entry body was hash-verified equal to its pre-move content (24 of 24 known
issues, every archived change-log and review-log heading accounted for in
exactly one place).

#### What did not change

No product behaviour, no contract, no schema, no test assertion, no gate.
Four source comments (`src/bin/atom-seed.rs`, `tests/claims_schema.rs`,
`tests/jobs_schema.rs`, `tests/seeder.rs`) were repointed from the old ledger
path to the archive path; the files compile and the suite is green. Nothing
was deleted: the archive is part of the record.

#### Effect on the working set

The documents a task reads by default drop from roughly 150K tokens (whole
logs plus mandated intake) to well under 40K; the archive holds the rest,
one link away. This entry is the first written under the new shape.

#### Documentation

This entry, `AGENTS.md` section 5, `docs/README.md` (the engineering index
rows and the archive row), `docs/engineering/implementation-status.md`
(the single-sourcing rule and the `K1` row), the archive set itself, and the
repointed references across `docs/` and the four source comments.

### ATOM-CHANGE-20261027-001 - Scoped API keys, so a machine can hold a credential

**Date:** 2026-10-27 (UTC)
**Type:** Feature - authentication
**Author:** the orchestrator with the `database`, `rust-backend`, `test-engineer` and `docs` subagents.
**Status:** Complete - **`Implemented`, not `Verified`.** The six gates are green and the feature is covered by eleven integration cases and thirteen unit tests. The `quality-gate` returned **PASS**; the `code-reviewer` read the diff three times and returned **FAIL on the documentation figures** each time, every finding is repaired, and no read of the final tree exists.

#### Why

The owner is integrating a deployed n8n instance with ATOM. ATOM had no
credential a workflow could hold: `AuthUser` accepted only a user JWT, whose
access token lives fifteen minutes and whose refresh token **rotates with reuse
detection**. A scheduled workflow that stores a refresh token therefore either
stops working the first time it rotates, or - if it retries a stale one -
triggers reuse detection and revokes every session of the account. That is the
actual failure this change removes, not a hypothetical one.

#### What changed

| Area | Change |
| ---- | ------ |
| Schema | `migrations/0007_api_keys.sql`: `api_keys (id, user_id, name, prefix, key_hash, last_used_at, created_at, revoked_at)`, three named `CHECK`s, `UNIQUE (key_hash)`, `INDEX api_keys_user_id_idx`, `ON DELETE CASCADE` from `users` |
| Credential | `atom_` + 32 bytes from the OS CSPRNG as lowercase hex - **69 characters**. `prefix` = `atom_` + the first 8 hex (13). Stored only as a **SHA-256** digest |
| Operations | `POST /api/v1/api-keys` (201, the secret returned once), `GET /api/v1/api-keys` (200, paginated, never a secret), `DELETE /api/v1/api-keys/{key_id}` (204, idempotent) |
| Authentication | `AuthUser::from_request_parts` branches on the `atom_` prefix before it reaches `TokenService`; a key resolves through one `JOIN` to exactly one `user_id` |
| Errors | unknown, malformed or revoked key -> `401 unauthorized`; the key of an inactive account -> `403 forbidden` `account_disabled`, the same answer that account's JWT already gets |
| Write cost | `last_used_at` is stamped by one statement that is a no-op inside the hour, so a hot key does not write on every request |
| Seeder | `api_keys` joins `USER_TABLES` (11 -> 12) so `V6` keeps proving the reset list against `information_schema`; no key is ever inserted |
| Surface | `ROUTE_TABLE` and the OpenAPI document move **51 -> 54** operations (52 under `/api/v1`, 2 unversioned probes) |

#### Two decisions worth not re-litigating

**SHA-256, and not Argon2.** A key is 256 bits of CSPRNG output rendered as
`atom_` plus 64 hex characters: there is no dictionary, no pattern and nothing
to guess, so a memory-hard function buys nothing. It is verified on **every
authenticated request**, and Argon2id at this project's password parameters
(19 MiB, two iterations, 8 MiB floor) costs tens of milliseconds and most of
20 MiB per verification - unauthenticated CPU and memory amplification against
our own API. The asymmetry with `users.password_hash` is the design. Recorded in
`ADR-0011`, in the migration header, and in `src/api_keys/secret.rs`.

**`401` for a revoked key, `403` for a disabled account.** One account state has
one answer. `authenticate_bearer` already returns `403 account_disabled` for the
JWT of an inactive account (`src/auth/service.rs:402`); a second credential type
that answered `401` for the same account would be a contract wart, so the locked
contract was amended before any code existed (ledger `A-C-K1-1`), and a test
asserts that the two credential kinds return the *same envelope*.

#### Checks, run on this tree

| Command | Result |
| ------- | ------ |
| `cargo fmt --all -- --check` | `0` |
| `cargo clippy --all-targets --all-features --locked -- -D warnings` | `0` |
| `cargo test --all-targets --locked` | `0` - **536 passed, 0 failed** on Windows (CI's Linux run adds the two unix-only `shutdown` tests, so it reports 538) |
| `cargo check --no-default-features --all-targets --locked` | `0` |
| `cargo run --quiet --bin atom-docs` | `0` |
| `cargo run --quiet --bin atom-docs -- --strict` | `0` |

`scripts/gate.ps1` runs exactly those six and reported `All gates passed.`

Hand-run against a live server on `atom_dev` (not a substitute for the suite; it
is what proved the ordering): login `200`; create `201` with a 69-character
secret; `GET /auth/me` with that key `200` for the creating account; listing
`200` with the prefix and `last_used_at` and no secret; revoke `204`; the same
key afterwards `401`; a second `DELETE` `204`; a foreign or unknown id `404`;
`atom_deadbeef` `401` and not `500`; an empty name and a 101-character name
`422` naming `name`; a disabled account's key `403` and its JWT `403` with an
identical body.

#### Limits this change does not remove

- A key has **no scope**: it carries the full authority of its owner and can
  mint another key, so revoking a leaked key does not by itself end access that
  key granted. `docs/security/threat-model.md` records it as `T-21`.
- A key does **not expire**. Revocation is the only end.
- `last_used_at` is coarse: a use within the hour of the last stamp is not
  recorded.
- Two schema tests (`tests/claims_schema.rs`, `tests/jobs_schema.rs`) pin the
  migration list by hand, so every new migration edits both. They now list
  `1..7`.

#### Documentation

`ADR-0011-api-keys.md` (new), `docs/api/README.md`, `docs/api/conventions.md`,
`docs/api/frontend-handover.md`, `docs/architecture/{security-architecture,
request-lifecycle,module-boundaries,system-overview,data-architecture}.md`,
`docs/database/{schema,seeding,migration-policy,README}.md`,
`docs/security/{README,threat-model,secure-development}.md`,
`docs/testing/strategy.md`, `docs/features/api-keys/status.md` (new),
`docs/features/README.md`, `docs/roadmap.md` and the root `README.md`.

**Correction (2026-10-29).** The status line and case counts of this entry were
edited in place during the repairs of 2026-10-27 ("ten integration cases" became
"eleven"), which deviated from this log's append-only rule; the deviation is
recorded here rather than hidden. The gap the entry records — no read of the
final tree — is closed by
[`ATOM-REVIEW-20261029-001`](review-log.md): `CODE_REVIEW` PASS,
`PRODUCT_CONTRACT` PASS WITH FINDINGS (three index rows corrected in the same
change), `NEW_REGRESSIONS` PASS, with all six gates at `0` on the same tree.
**The feature is `Verified` as of that review.**

