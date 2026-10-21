# ATOM change log

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-21
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

### ATOM-CHANGE-20261022-005 - The quality-gate findings, resolved

**Date:** 2026-10-22 (UTC)
**Type:** Defect fixes
**Author:** The ATOM engineering team - the orchestrator with the `quality-gate`
subagent
**Status:** Complete

#### Summary

The read-only `quality-gate` agent judged the delivery and returned **FAIL**,
with one blocking finding, seven required findings and four advisories. All of
them are addressed. The blocking finding was a false claim in the engineering
record: the OpenAPI document did not declare `401` on the two endpoints that
return it, while four documents asserted that it did.

#### Reason

[`../testing/verification-policy.md`](../testing/verification-policy.md)
requires a quality gate that judges the evidence rather than the summary of the
agent that produced it. The gate found exactly the class of defect the role
exists to catch: a record that claimed a verification that had not held.

#### Affected features and files

| Finding | Severity | Resolution |
| ------- | -------- | ---------- |
| The OpenAPI document omitted `401` on `POST /auth/login` and `POST /auth/refresh`, and no endpoint declared `408` | Blocking | Both endpoints answer `401` when the credentials or the refresh token are wrong, and the request timeout can produce a `408` on any endpoint. The annotations now declare both. |
| The CI pipeline was recorded as non-existent in four documents | Required | [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) exists; the status is now `Implemented` in the README, in [`../project-truth.md`](../project-truth.md) §17 and in [`implementation-status.md`](implementation-status.md). `KI-005` is Closed. |
| `implementation-status.md` said the change set had never been reviewed | Required | It now points at `ATOM-REVIEW-20261021-001`. |
| `testing/strategy.md` misstated the size of the suite and listed closed gaps | Required | 53 integration tests, not 30; the five closed gaps are removed and the remaining list is renumbered. |
| Two feature status files understated their own evidence | Required | The `PROJ-R02`, `TASK-R01` and `TASK-R04` rows now name the covering tests. |
| `security/README.md` claimed two controls had no test | Required | Both rows name the covering tests. |
| **The CI workflow would not have created the test database it points at** | Required | The official `postgres` image ignores `POSTGRES_MULTIPLE_DATABASES`. The workflow now creates `atom_test` with `psql` before the test step, and the compose file no longer relies on the non-existent variable. |
| Two records still described the rate limiter as sharing one bucket | Required | `Q-07` and `KI-012` now describe the post-`KI-010` behaviour. |
| Four advisory items | Advisory | A wrong document path, a mislabelled link, a stale sentence about the session revocation and a stale description of the rate-limiter row. |

#### Requirements and decisions referenced

| Reference | Subject |
| --------- | ------- |
| [`../project-truth.md`](../project-truth.md) §11.3 | `401` for invalid credentials. |
| [`../project-truth.md`](../project-truth.md) §7.8 | The OpenAPI document describes the API that actually exists. |
| [`../api/error-contract.md`](../api/error-contract.md) | The stable error codes and their status codes. |

#### Architectural impact

None. The route table, the public contracts and the schema are unchanged; the
OpenAPI document now describes them accurately.

#### Compatibility impact

The generated OpenAPI document now declares `401` on the login and refresh
operations, and `408` on every operation. A client generated from the
document will handle the `401` of a failed login and the `408` of a slow
request.

#### Security implications

None of the findings was a security defect. The rate limiter, the ownership
predicates and the secret handling were confirmed by the gate to be as
documented.

#### Tests and checks actually executed

| Check | Result |
| ----- | ------ |
| `cargo fmt --all -- --check` | Passed. |
| `cargo clippy --all-targets --all-features -- -D warnings` | Passed. |
| `cargo test --all-targets` | Passed: 39 unit, 26 authentication, 8 notes, 19 projects and tasks. 92 in total. |
| `cargo run --bin atom-docs` | Passed, no error and no warning. |
| `scripts/validate-opencode.ps1` | Passed, no warnings. |
| The generated OpenAPI document | 22 operations; `201` on the creates, `204` with no body on the deletes, `401` on `login` and `refresh`, `408` on every operation, and the bearer security scheme registered. |

#### Reviewer findings

Recorded in [`review-log.md`](review-log.md). The quality gate's own
limitations - it could not run `cargo run --bin atom-docs` or the OpenCode
validation script, because the read-only shell allow-list refuses them - are
recorded with its verdict.

---

### ATOM-CHANGE-20261021-004 - The independent review findings, resolved

**Date:** 2026-10-21 (UTC)
**Type:** Defect fixes
**Author:** The ATOM engineering team - the orchestrator with the `code-reviewer`
subagent
**Status:** Complete

#### Summary

The Phase 1 implementation was reviewed by the read-only `code-reviewer`
agent. The review returned **APPROVE WITH COMMENTS** and eleven findings. The
three Medium findings and the eight Low findings were all addressed. No
authorization bypass, no SQL injection, no secret leak, no `unwrap` in a
request path, no lock held across an `.await` and no PATCH-semantics defect
were found.

#### Reason

[`../testing/verification-policy.md`](../testing/verification-policy.md)
requires an independent review of a substantial change, and a recorded
resolution of every finding. The findings were real: the refresh-token reuse
detection was only half-implemented, and the request timeout answered with an
empty body while the published error contract promised the standard envelope.

#### Affected features and files

| Finding | Severity | Resolution |
| ------- | -------- | ---------- |
| Argon2id hashing ran on the Tokio worker thread | Medium | [`../../src/auth/service.rs`](../../src/auth/service.rs) moves hashing and verification to `tokio::task::spawn_blocking`. ADR-0002 calls the "no blocking work in a request path" rule load-bearing; it is now true. |
| Two concurrent refreshes of the same token both succeeded | Medium | [`../../src/auth/repository.rs`](../../src/auth/repository.rs) gains `claim`, an `UPDATE ... WHERE revoked_at IS NULL` that only one caller can win. The loser is treated as a reuse and every session of that user is revoked. |
| The 30-second timeout answered with an empty body | Medium | [`../../src/api/mod.rs`](../../src/api/mod.rs) replaces the `tower-http` timeout layer with a middleware that reports the timeout through the standard envelope. The timeout is now `APP_REQUEST_TIMEOUT_SECONDS`. |
| The OpenAPI document omitted the status codes the handlers return | Medium | Every `#[utoipa::path]` now declares `400`, `401`, `403`, `404`, `408`, `409`, `413`, `422`, `429` and `500` where they can occur. Two further passes were needed. The re-review found that the first pass keyed the response lists on the path alone, so the three `create` endpoints still declared `200` with a page body and the three `delete` endpoints declared `200` with a resource body; the lists are now keyed on the method and the path together. The quality gate then found that `login` and `refresh` still omitted `401`, which they do return when the credentials or the refresh token are wrong, and that no endpoint declared `408`, which the request timeout can produce anywhere. Both are now declared. |
| `revoke` was not scoped by the owner | Low | [`../../src/auth/repository.rs`](../../src/auth/repository.rs) adds `AND user_id = $2` to the predicate, and ADR-0005 now records the lookup exception explicitly. |
| The middleware-order comment was reversed | Low | [`../../src/api/mod.rs`](../../src/api/mod.rs) now states the effective order and points at the request-lifecycle document. |
| `hyper`, `http` and `subtle` were unused; `tower` and `http-body-util` were duplicated | Low | [`../../Cargo.toml`](../../Cargo.toml) no longer declares them. `tower` and `http-body-util` remain as dev-dependencies, which is where the test suite uses them. |
| A `CHECK` violation could log the failing row, including the password hash | Low | [`../../src/error.rs`](../../src/error.rs) logs only the SQLSTATE and the error kind for a database failure, and [`../../src/users/repository.rs`](../../src/users/repository.rs) maps a check violation to a `422` before it can reach the log. |
| The dummy-hash fallback was not a valid Argon2 hash | Low | [`../../src/auth/password.rs`](../../src/auth/password.rs) holds a valid `DUMMY_HASH` constant, so the timing equalisation always costs a real verification. |
| Unreferenced public helpers, and expired sessions were never pruned | Low | The unused helpers are removed. [`../../src/main.rs`](../../src/main.rs) runs a background task that prunes expired refresh sessions every hour. |
| `APP_MAX_BODY_BYTES` constrained nothing | Low | [`../../src/api/extract.rs`](../../src/api/extract.rs) reads the limit the router installs, so the configured value is the effective ceiling. |
| The dummy hash used cheaper parameters than a real hash, so the timing equalisation was only about a fifth effective | Medium | [`../../src/auth/password.rs`](../../src/auth/password.rs) derives the dummy hash from the configured cost parameters, so verifying against it costs exactly what verifying against a stored password costs. |
| The `SetRequestIdLayer` and `PropagateRequestIdLayer` were in the wrong order, so a *generated* correlation id was never echoed on the response | Medium | The setter is now the outer of the two, so it puts the id on the request before the propagator reads it. Covered by `a_generated_request_id_is_propagated_to_the_response`. |
| A timed-out response carried no correlation id | Low | The timeout is now inner to the request-id pair, so the `408` still carries the id. Covered by `a_slow_request_is_abandoned_with_the_error_envelope`. |
| The session-pruning task logged the raw driver message | Low | [`../../src/main.rs`](../../src/main.rs) logs only the SQLSTATE, like [`../../src/error.rs`](../../src/error.rs). |
| `claim` mutated a session without an owner predicate | Low | The predicate now carries `AND user_id = $2`, and ADR-0005 names it. |

#### Requirements and decisions referenced

| Reference | Subject |
| --------- | ------- |
| [`../project-truth.md`](../project-truth.md) §9 | The security expectations the fixes restore. |
| [`../decisions/ADR-0002-rust-axum-postgresql.md`](../decisions/ADR-0002-rust-axum-postgresql.md) | The rule that a request path performs no blocking work. |
| [`../decisions/ADR-0003-auth-token-design.md`](../decisions/ADR-0003-auth-token-design.md) | The refresh-token rotation and reuse-detection design. |
| [`../decisions/ADR-0005-ownership-enforced-in-sql.md`](../decisions/ADR-0005-ownership-enforced-in-sql.md) | The ownership predicate rule and its two documented exceptions. |
| [`../api/error-contract.md`](../api/error-contract.md) | The single error shape, including the `timeout` code. |

#### Architectural impact

None. The layering, the route table and the public contracts are unchanged.
The timeout became configurable, which adds one environment variable.

#### Compatibility impact

- The refresh-token rotation now fails closed: a second, concurrent
  exchange of the same token is refused and revokes the user's sessions. A
  client that exchanged the same refresh token twice in parallel now sees one
  success and one `401`, instead of two successes.
- The request timeout now answers with the standard envelope instead of an
  empty body. A client that parsed the body as JSON will now succeed where it
  previously failed to parse.
- `APP_MAX_BODY_BYTES` now bounds the request body. A client that sent more
  than one mebibyte used to be accepted up to four mebibytes and now receives
  `413`.

#### Security implications

Three of the fixes close security gaps: the blocking Argon2 work (a
denial-of-service amplifier), the refresh-token reuse race (a stolen token
could be raced against the legitimate client), and the database error log
(a `CHECK` violation message can contain the failing row, which for the
`users` table includes the password hash). The generic login failure is
unchanged.

#### Tests and checks actually executed

| Check | Result |
| ----- | ------ |
| `cargo fmt --all -- --check` | Passed. |
| `cargo clippy --all-targets --all-features -- -D warnings` | Passed. |
| `cargo test --all-targets` | Passed: 39 unit, 26 authentication, 8 notes, 19 projects and tasks. 92 in total. |
| `cargo run --bin atom-docs` | Passed, no errors and no warnings. |
| A running server | `/health/live`, `/health/ready`, registration, note creation, note listing, the OpenAPI document and the Swagger UI all answered. |

The new tests close the gaps the review identified: the concurrent refresh
race, the HTTP-level rate limit, the disabled account, the expired refresh
token, the expired access token, the per-user session revocation, the malformed
JSON body, the unknown query parameter value, the oversized body, the unmatched
route, the request-id echo, the generated request-id echo, the request timeout
and a task created as `done`.

#### Reviewer findings

The findings and their resolutions are recorded in
[`review-log.md`](review-log.md) under `ATOM-REVIEW-20261021-001`.

---

### ATOM-CHANGE-20261021-003 - The OpenCode engineering system, and the fixes it exposed

**Date:** 2026-10-21 (UTC)
**Type:** Engineering system, with source fixes
**Author:** The ATOM engineering team - the orchestrator with the specialist
agents defined in [`.opencode/agents/`](../../.opencode/agents/)
**Status:** Complete

#### Summary

The repository was given a complete OpenCode engineering system: a project
configuration, an orchestrator and nine specialists, three commands, three
skills, two validation scripts and a contributor guide. Building and verifying
it exposed four defects in the Phase 1 code and in the documentation, all of
which are fixed by this change.

#### Reason

[`../project-truth.md`](../project-truth.md) A13 requires the quality gates to
be real, and [`../testing/verification-policy.md`](../testing/verification-policy.md)
requires that a result is only claimed when it was observed. The same
standard applies to the agent system: a configuration that parses is not a
configuration that works. Every permission rule, every agent and every
documented behaviour was therefore verified against the installed OpenCode
CLI rather than against the schema alone.

#### Affected features and files

| Area | Change |
| ---- | ------ |
| [`../../opencode.jsonc`](../../opencode.jsonc) | The project configuration: the default model, the ordered permission rules, the policies, the ambient instructions, the watcher, compaction and the experimental settings. |
| [`../../AGENTS.md`](../../AGENTS.md) | The instructions every agent reads, including the mandatory documentation intake protocol and the status vocabulary. |
| [`../../.opencode/agents/`](../../.opencode/agents/) | Ten agents: the orchestrator, the architect, the Rust backend, authentication and security, the database, the test engineer, the code reviewer, the quality gate, the documentation specialist and the DevOps engineer. The reviewer and the quality gate are read-only. |
| [`../../.opencode/commands/`](../../.opencode/commands/) | `/feature`, `/review` and `/verify`. |
| [`../../.opencode/skills/`](../../.opencode/skills/) | The `atom-delivery`, `atom-docs` and `atom-specialist` workflows. |
| [`../../scripts/`](../../scripts/) | `validate-opencode.ps1`, `validate-opencode.sh`, `check-docs.ps1`, `check-docs.sh`. |
| [`../contributing-opencode.md`](../contributing-opencode.md) | The contributor guide: the model override ladder, the permission model, and the verified behaviour of the installed OpenCode version. |
| [`../../src/main.rs`](../../src/main.rs) | **Defect fix.** The server now installs the connection information, so the rate limiter is keyed by the client address. Closes `KI-010`. |
| [`../../tests/common/mod.rs`](../../tests/common/mod.rs), [`../../tests/auth_api.rs`](../../tests/auth_api.rs) | **Defect fix.** An HTTP-level test of the rate limiter. Closes `KI-002`. |
| [`../../.env.example`](../../.env.example) | **Defect fix.** The description of the test harness now matches what it does. Closes `KI-011`. |
| [`../project-truth.md`](../project-truth.md), [`../../CONTRIBUTING.md`](../../CONTRIBUTING.md) | **Defect fix.** Corrected links and the missing contributor guide. Closes `KI-013`. |
| [`../../src/bin/atom-docs.rs`](../../src/bin/atom-docs.rs) | The documentation checker: required documents, relative links, feature status vocabulary, ADR identifiers and status, change identifiers, feature directories. |
| [`../../.github/workflows/ci.yml`](../../.github/workflows/ci.yml) | The pipeline: formatting, linting, tests, a release build, the documentation check and the OpenCode configuration check. |

#### Requirements and decisions referenced

| Reference | Subject |
| --------- | ------- |
| [`../project-truth.md`](../project-truth.md) A13 | The quality gates and the rule that a result is only claimed when observed. |
| [`../project-truth.md`](../project-truth.md) A9 | The security expectations that the permission model implements. |
| [`../decisions/ADR-0004-documentation-as-project-truth.md`](../decisions/ADR-0004-documentation-as-project-truth.md) | The documentation tree is part of the product. |

#### Architectural impact

None. The application architecture is unchanged. The agent system is
development infrastructure and ships no runtime code.

#### Compatibility impact

None for the API. The change adds a CI workflow that does not exist yet and
corrects the description of the test database in `.env.example`.

#### Security implications

The permission model is the enforcement point for the "routine local
development is autonomous, consequential actions are controlled" rule. Two
things were verified by running the CLI rather than by reading the schema:

1. The **top-level** `policies` array is ignored by OpenCode v2.0.27. The
   same statements under `experimental.policies` do block the command. The
   committed configuration therefore keeps its policies under
   `experimental.policies`, and
   [`../contributing-opencode.md`](../contributing-opencode.md) records the
   finding.
2. An **unquoted** `color: #3b82f6` in an agent's frontmatter is parsed as a
   value followed by a YAML comment. The colour becomes empty, the agent
   fails validation, and OpenCode silently drops it. Every agent colour is
   now quoted.

#### Tests and checks actually executed

| Check | Result |
| ----- | ------ |
| `cargo fmt --all -- --check` | Passed. |
| `cargo clippy --all-targets --all-features -- -D warnings` | Passed. |
| `cargo test --all-targets` | Passed: 39 unit, 14 authentication, 8 notes, 9 projects and tasks. 70 in total. |
| `cargo run --bin atom-docs` | The documentation checker. Its result is reported in the section below. |
| `opencode debug config` | The project configuration is loaded and merged. `experimental.policies` is present in the resolved configuration. |
| `opencode debug agents` | All ten project agents are discovered, with the expected modes and permissions. |
| `scripts/validate-opencode.ps1` | Passed, no warnings. |
| `scripts/validate-opencode.sh` | Passed, no warnings. |
| A running server | `/health/live`, `/health/ready`, registration, note creation, note listing, the OpenAPI document and the Swagger UI all answered. Twelve login attempts produced ten `401` responses and then `429` with a `retry-after` header. |
| A policy probe | A `tool.use` deny statement blocked the command under `experimental.policies` and did nothing at the top level. |

#### Reviewer findings

No independent review has been performed on this change set yet;
[`review-log.md`](review-log.md) records that explicitly.

---

### ATOM-CHANGE-20261021-002 — Phase 1 security, testing and engineering record

**Date:** 2026-10-21 (UTC)
**Type:** Documentation
**Author:** The ATOM engineering team — the orchestrator with the specialist
agents defined in [`.opencode/agents/`](../../.opencode/agents/)
**Status:** Complete

#### Summary

The security, testing and engineering-record part of the knowledge base was
written: the security index, the threat model and the secure-development
rules; the testing strategy and the verification policy; the change log, the
implementation status, the review log, the known issues and the open
questions; the roadmap; and the five record templates.

#### Reason

`AGENTS.md` §5 makes documentation part of the delivery, and
[`ADR-0004-documentation-as-project-truth.md`](../decisions/ADR-0004-documentation-as-project-truth.md)
makes the documentation tree part of the product. A change set that is not
described cannot be reviewed, and a control that is not written down cannot be
checked by the next contributor.

#### Affected features and files

No behaviour changed. This entry changes documentation only.

| Document | Subject |
| -------- | ------- |
| [`../roadmap.md`](../roadmap.md) | The future features, their status, their dependencies and the Phase 1 boundary. |
| [`../security/README.md`](../security/README.md), [`../security/threat-model.md`](../security/threat-model.md), [`../security/secure-development.md`](../security/secure-development.md) | The security posture, the threat model and the rules a change must follow. |
| [`../testing/strategy.md`](../testing/strategy.md), [`../testing/verification-policy.md`](../testing/verification-policy.md) | The test layers and what must pass before a change is complete. |
| [`../engineering/`](../engineering/) | This record: changes, status, reviews, known issues, open questions. |
| [`../templates/`](../templates/) | The fill-in templates for a feature specification, an ADR, a change record, a review record and a verification record. |

The architecture, decision, feature, API and database documents were written in
the same effort by the other specialists and are not itemised here.

#### Requirements and decisions referenced

| Reference | Subject |
| --------- | ------- |
| [`../project-truth.md`](../project-truth.md) §5 | The roadmap items and their dependencies. |
| [`../project-truth.md`](../project-truth.md) §6 | The explicit Phase 1 exclusions. |
| [`../project-truth.md`](../project-truth.md) §9 | The security and privacy expectations. |
| [`../project-truth.md`](../project-truth.md) §13 | The testing and quality requirements. |
| [`../project-truth.md`](../project-truth.md) §16 | The decisions awaiting confirmation. |
| [`ADR-0004-documentation-as-project-truth.md`](../decisions/ADR-0004-documentation-as-project-truth.md) | The documentation tree is part of the product. |

#### Architectural impact

None. No code, schema, contract or configuration changed.

#### Compatibility impact

None. No public contract changed.

#### Security implications

None. No control was added, removed or altered. The security documents
describe the controls that already existed and record their limits; the
limits themselves are unchanged and are listed in
[`known-issues.md`](known-issues.md).

#### Checks and results

All four commands were run on 2026-10-21 against the tree that contains this
entry, with
`$env:TEST_DATABASE_URL = "postgres://atom:atom@127.0.0.1:5432/atom_test"`.

| Command | Result |
| ------- | ------ |
| `cargo fmt --all -- --check` | **Failed.** Six formatting differences, all in [`../../src/bin/atom-docs.rs`](../../src/bin/atom-docs.rs), which was edited concurrently by another agent and is not part of this entry. This entry changed no source file. |
| `cargo clippy --all-targets --all-features -- -D warnings` | **Failed.** One error: `clippy::collapsible_if` in [`../../src/bin/atom-docs.rs`](../../src/bin/atom-docs.rs), the same concurrent edit. This entry changed no source file. |
| `cargo test --all-targets` | **Passed.** 69 tests: 39 unit tests, 13 authentication integration tests, 8 notes integration tests, 9 projects/tasks integration tests. |
| `cargo run --bin atom-docs` | **Failed with 76 errors.** Every error is a broken link to a document that is required by [`../README.md`](../README.md) but was not yet in the tree when the check ran: `docs/architecture/…` and `docs/decisions/…`. Two further errors are pre-existing and are recorded as `KI-013` in [`known-issues.md`](known-issues.md): a wrong path prefix in [`../project-truth.md`](../project-truth.md) and a missing target in [`../../CONTRIBUTING.md`](../../CONTRIBUTING.md). No error points at a document written by this entry other than at the two pending directories named above. |

The two failing code gates are caused by a file outside this entry's scope.
They are reported here rather than worked around, and the fix belongs to the
owner of that file.

#### Manual verification

Every control, test name, configuration key and command in the documents
written by this entry was read out of the source before it was written, and
every test name quoted in the documents was matched against the test files.
The test counts were confirmed by running the suite.

#### Reviewer findings

**No independent review has been performed on this change set.** See
[`review-log.md`](review-log.md).

#### Documentation

This entry is itself part of the documentation it describes.
### ATOM-CHANGE-20261021-001 — Phase 1 backend foundation

**Date:** 2026-10-21 (UTC)
**Type:** Foundation
**Author:** The ATOM engineering team — the orchestrator with the specialist
agents defined in [`.opencode/agents/`](../../.opencode/agents/)
**Status:** Complete

#### Summary

The Phase 1 backend was built: a Rust/Axum modular monolith on PostgreSQL
through SQLx, with a typed configuration read once at start-up, a central error
contract, a complete authentication and authorization boundary, notes, projects
and tasks, a versioned REST API under `/api/v1` with a generated OpenAPI
document and a local API explorer, health endpoints, structured logging with
request correlation, and a documentation tree.

#### Reason

ATOM needs a backend foundation before any client exists. The requirements are
the accepted items in [`../project-truth.md`](../project-truth.md) §4, and the
decision to build it as a modular monolith rather than a set of services is
recorded in
[`ADR-0001-modular-monolith.md`](../decisions/ADR-0001-modular-monolith.md).

#### Affected features and files

| Feature | Area | Files |
| ------- | ---- | ----- |
| Foundation | Configuration, error contract, pagination, state, telemetry, health | [`../../src/config.rs`](../../src/config.rs), [`../../src/error.rs`](../../src/error.rs), [`../../src/pagination.rs`](../../src/pagination.rs), [`../../src/state.rs`](../../src/state.rs), [`../../src/telemetry.rs`](../../src/telemetry.rs), [`../../src/health.rs`](../../src/health.rs), [`../../src/main.rs`](../../src/main.rs), [`../../src/lib.rs`](../../src/lib.rs) |
| Foundation | HTTP layer | [`../../src/api/mod.rs`](../../src/api/mod.rs), [`../../src/api/routes.rs`](../../src/api/routes.rs), [`../../src/api/extract.rs`](../../src/api/extract.rs), [`../../src/api/middleware.rs`](../../src/api/middleware.rs) |
| Authentication | Registration, login, refresh, logout, `/auth/me` | [`../../src/auth/`](../../src/auth/), [`../../src/users/`](../../src/users/) |
| Notes | CRUD, archive, search, pagination | [`../../src/notes/`](../../src/notes/) |
| Projects and tasks | CRUD, status, priority, filtering, ordering, cascade | [`../../src/projects/`](../../src/projects/), [`../../src/tasks/`](../../src/tasks/) |
| Security | Rate limiting | [`../../src/ratelimit.rs`](../../src/ratelimit.rs) |
| Database | Schema | [`../../migrations/0001_users_and_sessions.sql`](../../migrations/0001_users_and_sessions.sql), [`../../migrations/0002_notes.sql`](../../migrations/0002_notes.sql), [`../../migrations/0003_projects_and_tasks.sql`](../../migrations/0003_projects_and_tasks.sql) |
| Tests | Unit and integration suites | [`../../src/`](../../src/), [`../../tests/common/mod.rs`](../../tests/common/mod.rs), [`../../tests/auth_api.rs`](../../tests/auth_api.rs), [`../../tests/notes_api.rs`](../../tests/notes_api.rs), [`../../tests/projects_tasks_api.rs`](../../tests/projects_tasks_api.rs) |
| Tooling | Documentation checker, scripts, agent configuration | [`../../src/bin/atom-docs.rs`](../../src/bin/atom-docs.rs), [`../../scripts/`](../../scripts/), [`../../opencode.jsonc`](../../opencode.jsonc) |

#### Requirements and decisions referenced

| Reference | Subject |
| --------- | ------- |
| [`../project-truth.md`](../project-truth.md) §4.1 | Foundation: modular monolith, SQLx persistence, typed configuration, the error contract, structured logging, health endpoints, the versioned API, the OpenAPI document, the documentation. |
| [`../project-truth.md`](../project-truth.md) §4.2 | Authentication and authorization: registration, Argon2id, the password policy, access tokens, refresh tokens, reuse detection, logout, ownership checks, rate limiting, the safe user representation. |
| [`../project-truth.md`](../project-truth.md) §4.3 | Notes: CRUD, partial update, archiving, search, pagination, isolation. |
| [`../project-truth.md`](../project-truth.md) §4.4 | Projects and tasks: CRUD, filtering, ordering, completion timestamps, the project delete behaviour. |
| [`../project-truth.md`](../project-truth.md) §4.5 | Quality: unit tests, integration tests, contract tests, security regression tests, the three gates, the disposable test database. |
| [`ADR-0001-modular-monolith.md`](../decisions/ADR-0001-modular-monolith.md) | ATOM is a modular monolith. |
| [`ADR-0002-rust-axum-postgresql.md`](../decisions/ADR-0002-rust-axum-postgresql.md) | The technology stack. |
| [`ADR-0003-auth-token-design.md`](../decisions/ADR-0003-auth-token-design.md) | The access and refresh token design. |
| [`ADR-0004-documentation-as-project-truth.md`](../decisions/ADR-0004-documentation-as-project-truth.md) | The documentation tree is part of the product. |
| [`ADR-0005-ownership-enforced-in-sql.md`](../decisions/ADR-0005-ownership-enforced-in-sql.md) | Authorization is enforced in SQL, not in handlers. |

#### Architectural impact

This is the change that establishes the architecture, so the impact is
foundational rather than incremental.

- A modular monolith with the fixed layering handlers → services →
  repositories → PostgreSQL. Every feature module keeps the same five
  layers.
- A typed `Config` is read once at start-up and passed down explicitly.
  `AppState` holds it and every service, and there is no global mutable
  state.
- One error type, `AppError`, with one public envelope and a stable error
  code. Handlers never build a response by hand.
- The request pipeline is explicit and ordered: panic catch, request
  correlation, timeout, CORS, tracing, body limit, route.
- The schema is owned by append-only migrations that are embedded in the
  binary, so a deployed instance never depends on files shipped next to it.
- The OpenAPI document is generated from the handlers, so it cannot describe a
  route that does not exist.

#### Compatibility impact

None. This is the first implementation; there is no earlier contract to keep
compatible with. The only externally visible surface is the `/api/v1` prefix
chosen here, which is versioned from the start precisely so that a future
change can be introduced without breaking an existing client.

#### Security implications

This change introduces the whole security boundary:

- Argon2id password hashing with a 16-byte random salt and configurable cost
  parameters; a 12–256 character password policy.
- Short-lived HS256 access tokens validated for signature, expiry, issuer and
  audience; a signing secret of at least 32 bytes with the documented
  placeholder refused in production; the secret and the database URL redacted
  from every `Debug` rendering.
- Opaque refresh tokens of which only the SHA-256 hash is stored, rotated on
  use, with reuse of an already-rotated token revoking every session of that
  account.
- A generic authentication failure, with a dummy Argon2 verification so that
  an unknown address does not answer faster than a wrong password.
- Ownership enforced in SQL on every query that touches user data, with `404`
  rather than `403` for a resource the caller does not own.
- A fixed error envelope that never carries database or internal detail.
- A per-process rate limit on registration, login and refresh, with a `429`
  and a `retry-after` header.
- An explicit CORS allowlist, a body-size limit, a request timeout and a
  panic handler.

The limitations of this boundary — the per-process limiter, the shared
rate-limit key, the ignored `X-Forwarded-For`, the run-time SQLx check — are
recorded in [`known-issues.md`](known-issues.md), not hidden.

#### Checks and results

| Command | Result |
| ------- | ------ |
| `cargo fmt --all -- --check` | **Passed.** No formatting differences. |
| `cargo clippy --all-targets --all-features -- -D warnings` | **Passed.** No lint findings, no suppressed lint. |
| `cargo test --all-targets` | **Passed.** 69 tests: 39 unit tests, 13 authentication integration tests, 8 notes integration tests, 9 projects/tasks integration tests. The integration tests ran against a disposable PostgreSQL database. |

**Status note (added 2026-10-21, later the same day).** The tree changed
after this entry was written: another agent edited
[`../../src/bin/atom-docs.rs`](../../src/bin/atom-docs.rs) during the same
working session. At the last run, `cargo fmt --all -- --check` and
`cargo clippy --all-targets --all-features -- -D warnings` both fail on that
one file, while `cargo test --all-targets` still passes with 69 tests. The
results above remain the record of the state at the time of the foundation
work; the current state is recorded in `ATOM-CHANGE-20261021-002`.

#### Manual verification

The server was started and the following were exercised successfully:
`/health/live`, `/health/ready`, user registration, note creation, note
listing, the OpenAPI document at `/api/v1/openapi.json`, and the API explorer
at `/api/v1/docs/`.

#### Reviewer findings

**No independent review has been performed on this change set.** The
`code-reviewer` and the `quality-gate` agents are read-only and had not been
run against the foundation when this entry was written, so this section is
empty by fact and not by omission. The empty record and the format for a
future review are in [`review-log.md`](review-log.md).

#### Documentation

The knowledge base was created with the implementation: the requirements in
[`../project-truth.md`](../project-truth.md), the architecture and decision
documents under [`../architecture/`](../architecture/) and
[`../decisions/`](../decisions/), the feature documents under
[`../features/`](../features/), the API contract under [`../api/`](../api/),
the schema under [`../database/`](../database/), the security model under
[`../security/`](../security/), the test approach under
[`../testing/`](../testing/), and this engineering record.

