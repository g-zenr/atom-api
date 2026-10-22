# ATOM Phase 1 — code audit report

**Date:** 2026-10-22
**Scope:** the whole repository at `aba97f6` plus the working tree
**Audits:** five (duplication, standards and architecture, documentation,
efficiency, configuration and reusability)
**Baseline before the audit:** `cargo fmt --check` clean, `cargo clippy
--all-targets --all-features -- -D warnings` clean, 133 tests passing,
`cargo run --bin atom-docs` clean, `scripts/validate-opencode.ps1` clean.

The five audits were run independently, read-only, against the same tree. Their
findings are consolidated below. Where two audits found the same defect, the
finding is listed once and both identifiers are given, because an independent
second opinion is evidence and not noise.

Every finding carries: category, severity, priority, the files and symbols
involved, the evidence, why it matters, the fix, the dependencies, the
regression risk, the tests it needs, the documentation it needs, and its
status (confirmed, suspected, or needs investigation).

---

## 1. Summary

| Severity | Count | Notable |
| -------- | ----- | ------- |
| Critical | 0 | — |
| High | 2 | F-01, F-02 |
| Medium | 19 | F-03 … F-22 |
| Low | 62 | F-23 … F-83 |
| Retained by design | 11 | R-01 … R-11 (§4) |

No Critical finding was confirmed. The two High findings are both real
behavioural defects in code that had been written and reviewed earlier:

- **F-01** — the configured pagination bounds are validated, published and
  then ignored, so a setting an operator can change has no effect.
- **F-02** — a refresh-token rotation recomputes the session deadline from
  "now", which silently converts the absolute lifetime that ADR-0003 requires
  into a sliding one.

`F-01` was found by three of the five audits independently; `F-02`
by one.

### By audit category

| Category | Findings | Highest severity |
| -------- | -------- | ---------------- |
| Redundant, duplicated and unnecessary code | F-04, F-06, F-07, F-08, F-23, F-24 … F-42 | Medium |
| Standards, architecture and best practices | F-02, F-20, F-22, F-52, F-53 | Medium |
| Documentation completeness and accuracy | F-09, F-10, F-16 … F-19, F-21, F-43, F-54 … F-72 | High |
| Code efficiency and resource usage | F-44 … F-51 | Low |
| Configuration and reusability | F-01, F-03, F-05, F-11 … F-15, F-73 … F-83 | High |

### Prioritisation

1. **Critical security and data integrity** — none found.
2. **High-severity correctness and reliability** — F-01, F-02.
3. **High-severity security and authorization** — none found. The
   authentication boundary, the ownership predicates and the secret handling
   were checked against the source and hold.
4. **Foundational configuration and architecture** — F-01, F-11 … F-15, F-20,
   F-22, F-24.
5. **Duplication and maintainability** — F-04, F-06 … F-08, F-23 … F-42.
6. **Efficiency** — F-44 … F-51.
7. **Documentation** — F-09, F-10, F-16 … F-19, F-43, F-54 … F-72.
8. **Cleanup** — the remainder.

---

## 2. The findings

### 2.1 High

```text
[F-01] The configured pagination bounds are validated, published and ignored
  category:   configuration / duplication
  severity:   High
  priority:   1
  files:      config/api.toml:24-32
              src/config/settings.rs:165-175
              src/config/validation.rs:143-160
              src/pagination.rs:11-13, 29-35
              src/notes/service.rs:61, src/projects/service.rs:60,
              src/tasks/service.rs:87
  evidence:   `effective_limit()` clamps against the compile-time constants
              `DEFAULT_LIMIT` and `MAX_LIMIT`; the configured
              `api.pagination.default_limit` and `api.pagination.max_limit`
              are read into the typed settings, validated at start-up and
              published in the operator reference, but no request path reads
              them. A repository-wide search for `api.pagination` matches only
              the settings, the validation and the configuration tests.
  impact:     A setting an operator can change has no effect. An operator who
              sets `max_limit = 50` still receives 100 rows, and nothing
              warns them: the start-up validation passes and the value is
              echoed back by the configuration. The published client
              convention is defined in two places that can drift, and the one
              ADR-0006 designates as authoritative is inert.
  fix:        Give `PaginationQuery::effective_limit` the configured bounds
              and hold them in the three services.
  depends:    —
  risk:       The three list endpoints change what they return if the
              configured values ever differ from 20/100. The published
              contract in `docs/api/conventions.md` stays 20/100, and the
              setting has no environment variable, so the default deployment
              is unaffected.
  tests:      A unit test that a configured maximum is honoured, plus an
              integration test that a larger `limit` is clamped.
  docs:       `docs/api/conventions.md`, `docs/architecture/deployment.md`,
              `config/README.md`, ADR-0006.
  status:     confirmed (Audit 1 H1, Audit 4 A-06, Audit 5 CFG-01)
```

```text
[F-02] A refresh-token rotation extends the absolute lifetime
  category:   correctness / security
  severity:   High
  priority:   1
  files:      src/auth/service.rs:215-230 (refresh), :296-318 (issue_token_pair)
              src/auth/models.rs:13
              docs/decisions/ADR-0003-auth-token-design.md:76-78, 173
  evidence:   The rotation writes the replacement row with
              `expires_at = self.refresh_expiry()`, which is
              `Utc::now() + refresh_ttl`. ADR-0003 line 173 states "There is
              one absolute refresh lifetime and no idle timeout", and lines
              76-78 give the lifetime as 14 days. The documentation therefore
              promises an absolute lifetime and the code implements a sliding
              one.
  impact:     A client that keeps rotating its refresh token holds a usable
              session for ever. The reuse detection still fires for a
              *revoked* row, so this is not a complete bypass of the reuse
              protection, but the absolute lifetime — the property the ADR
              names — does not hold.
  fix:        The replacement session inherits the deadline of the session it
              replaces.
  depends:    —
  risk:       A session that was close to its deadline now expires on
              schedule instead of being extended. That is the intended
              behaviour, and the reuse-detection tests still pass because
              the revoked row keeps its own deadline.
  tests:      An integration test that rotates a token and asserts that the
              replacement row carries the same `expires_at`.
  docs:       `docs/architecture/data-architecture.md`,
              `docs/features/authentication/*`,
              `docs/engineering/known-issues.md`.
  status:     confirmed (Audit 2 F-13)
```

### 2.2 Medium

```text
[F-03] `app.name` is a documented setting that nothing reads
  category:   configuration
  severity:   Medium   priority: 4
  files:      config/app.toml:13-15, src/config/settings.rs:54-55, 343-345,
              src/config/tests.rs:339, config/README.md:81,
              docs/architecture/deployment.md:74
  evidence:   The OpenAPI title comes from `api.title`
              (`src/api/routes.rs:117`); the start-up log uses a literal
              (`src/main.rs:34`). The only reader of `app.name` is a test.
              The reference tables claim it drives both.
  impact:     A committed, documented setting that does nothing.
  fix:        Delete the key, the field and the default.
  depends:    —      risk: Low.
  tests:      Update the configuration test that asserted it.
  docs:       config/README.md, docs/architecture/deployment.md.
  status:     confirmed (Audit 1 M1, Audit 5 CFG-02)
```

```text
[F-04] `field_error` is implemented twice, identically
  category:   duplication
  severity:   Medium   priority: 5
  files:      src/helpers/validation.rs:85-88, src/users/service.rs:184-188
  evidence:   The same four-line private function in two modules.
  impact:     The shape of a field violation is defined in two places; a
              change to the envelope would have to be made twice.
  fix:        Make the shared helper public and import it.
  depends:    —      risk: None; a pure move.
  tests:      The existing unit tests cover both call sites.
  docs:       docs/architecture/module-boundaries.md.
  status:     confirmed (Audit 1 M5, Audit 5 REU-01)
```

```text
[F-05] `messages.general.body_mismatch` is a catalogue key nobody reads
  category:   configuration
  severity:   Medium   priority: 4
  files:      config/messages.toml:40-41, src/config/messages.rs:77
  evidence:   The extractor reports a contract mismatch as a generic
              validation failure (`src/api/extract.rs:76-80`). The catalogue
              claims to hold "every string that reaches a client".
  impact:     A catalogue entry that can never reach a client.
  fix:        Remove the key.
  depends:    —      risk: Low; no response body changes.
  tests:      The catalogue completeness test.
  docs:       config/README.md §5.
  status:     confirmed (Audit 5 CFG-03, Audit 3 A3-29)
```

```text
[F-06] The message-catalogue test fixture is copied into five modules
  category:   duplication / test setup
  severity:   Medium   priority: 5
  files:      src/helpers/validation.rs, src/notes/service.rs,
              src/projects/service.rs, src/tasks/service.rs,
              src/users/service.rs
  evidence:   Five identical `fn messages()` helpers that parse
              `config/messages.toml` with `include_str!`.
  impact:     A rename or a move of the catalogue breaks five places.
  fix:        One shared fixture.
  depends:    —      risk: Low; test-only.
  tests:      —      docs: —
  status:     confirmed (Audit 1 M6, Audit 5 REU-03)
```

```text
[F-07] `RequestSettings` duplicates `RequestLimits` and carries a dead field
  category:   duplication
  severity:   Medium   priority: 5
  files:      src/api/mod.rs:60-66, 73-76, 107-109, 128
  evidence:   The struct copies two fields of
              `crate::config::settings::RequestLimits`; its
              `max_body_bytes` is never read, because the router installs the
              limit directly from the configuration.
  impact:     A second, partial copy of the request limits that can drift
              from the live one without any signal.
  fix:        Drop the struct; let the request context hold the configured
              limits.
  depends:    —      risk: The type is public API of the crate.
  tests:      The timeout test in `tests/projects_tasks_api.rs`.
  docs:       docs/architecture/module-boundaries.md, request-lifecycle.md.
  status:     confirmed (Audit 1 M3, Audit 5 CFG-19)
```

```text
[F-08] Dead public surface: four methods, one helper and one catalogue key
  category:   dead code
  severity:   Medium   priority: 5
  files:      src/state.rs:115-118 (`sessions`), src/ratelimit.rs:88-93
              (`reset`), src/config/loader.rs:108-112 (`directory`),
              src/utils/text.rs:12-15 (`is_blank`),
              src/state.rs:39-41 (`tokens`, `passwords`)
  evidence:   No caller anywhere in `src/` or `tests/`. `RateLimiter::reset`
              claims "Used by tests" and no test calls it.
  impact:     The public surface advertises capabilities that do not exist.
              `AppState::sessions()` builds a repository per call, which is
              the opposite of the invariant the module boundary document
              states.
  fix:        Delete them, or use them where the rule is written inline.
  depends:    —      risk: `AppState`'s fields are public on a library crate.
  tests:      —      docs: docs/architecture/module-boundaries.md.
  status:     confirmed (Audit 1 L1, Audit 2 F-09, Audit 3 A3-21,
              Audit 5 REU-02, REU-08)
```

```text
[F-09] The operator reference names two settings that do not exist
  category:   documentation
  severity:   Medium   priority: 4
  files:      docs/architecture/deployment.md:101-102
  evidence:   The table lists `auth.jwt_issuer` and `auth.jwt_audience`; the
              keys are `auth.issuer` and `auth.audience`
              (`config/auth.toml:17,20`). Every settings structure refuses
              unknown keys, so an operator who follows the reference gets a
              start-up parse error.
  impact:     The operator's contract names a setting that cannot be set.
  fix:        Correct the two rows, and make the checker compare the keys.
  depends:    —      risk: None.
  tests:      A checker assertion, added as part of F-14.
  docs:       docs/architecture/deployment.md.
  status:     confirmed (Audit 1 M4, Audit 3 A3-01, Audit 5 CFG-09)
```

```text
[F-10] The published API conventions describe a body cap the code does not have
  category:   documentation
  severity:   Medium   priority: 4
  files:      docs/api/conventions.md:200-207
  evidence:   The document describes `config.max_body_bytes` (the path does
              not exist) and a 4 MiB backstop in the extractor; the extractor
              passes `usize::MAX` and the path is
              `config.app.limits.max_body_bytes`.
  impact:     Two of the three documents that describe the body limit
              describe different limits.
  fix:        Rewrite §9 to match the code and the two sibling documents.
  depends:    —      risk: None.
  tests:      The body-limit test in `tests/auth_api.rs`.
  docs:       docs/api/conventions.md.
  status:     confirmed (Audit 2 F-03, Audit 3 A3-02, Audit 4 A-08,
              Audit 5 CFG-10)
```

```text
[F-11] The password minimum has no floor
  category:   configuration
  severity:   Medium   priority: 4
  files:      src/config/validation.rs:271-277,
              docs/features/authentication/specification.md:105
  evidence:   The only rule is `min_length == 0`. The accepted requirement is
              a password of at least twelve characters. The Argon2 memory
              floor *is* enforced, so the pattern exists in the codebase.
  impact:     A configuration file can weaken an accepted product requirement
              and the process starts happily.
  fix:        A floor constant next to the Argon2 floor.
  depends:    —      risk: A deployment that already lowered it below twelve
              is now refused at start-up. That is the point.
  tests:      A configuration test for the rule.
  docs:       config/README.md, docs/architecture/deployment.md.
  status:     confirmed (Audit 5 CFG-04)
```

```text
[F-12] The clock-skew allowance has no upper bound
  category:   configuration
  severity:   Medium   priority: 4
  files:      src/config/validation.rs (no rule),
              src/auth/token.rs:141
  evidence:   The value is applied verbatim as the JWT leeway. No rule
              touches it.
  impact:     A one-year allowance makes an expired access token valid for a
              year, while the configuration still reads as a clock-skew
              allowance.
  fix:        An upper bound.
  depends:    —      risk: A deployment with genuinely drifting hosts is
              refused; the message must name the bound.
  tests:      A configuration test for the rule.
  docs:       config/README.md, docs/architecture/deployment.md.
  status:     confirmed (Audit 5 CFG-05)
```

```text
[F-13] The prune grace is documented as a security window but has no floor,
        and the ADR it points at does not mention it
  category:   configuration / documentation
  severity:   Medium   priority: 4
  files:      config/app.toml:39-45, src/config/loader.rs:297-300,
              src/config/validation.rs:135-143
  evidence:   The file says the grace "is recorded in ADR-0003"; ADR-0003
              contains no mention of the grace, the prune interval or 86400.
              The only rule is `> 0`.
  impact:     A false cross-reference gives false assurance, and the window
              can be shortened to nothing.
  fix:        A floor, and a correct cross-reference.
  depends:    —      risk: A deployment that set it below the floor is refused.
  tests:      A configuration test for the rule.
  docs:       config/app.toml, config/README.md,
              docs/architecture/deployment.md.
  status:     confirmed (Audit 5 CFG-06)
```

```text
[F-14] The documentation checker cannot see the configuration surface
  category:   tooling / configuration
  severity:   Medium   priority: 4
  files:      src/bin/atom-docs.rs:66-74, 458-467
  evidence:   The checker compares `.env.example` with the loader, but filters
              the loader's variables by the `APP_` and `DATABASE_` prefixes,
              so `ATOM_CONFIG_DIR` is excluded from **both** directions. It
              never compares the keys in `config/*.toml` with the reference
              tables, and it does not require
              `config/environments/production.toml`.
  impact:     The gate that exists to catch exactly these defects is blind to
              them, and reports OK while they are present.
  fix:        Widen the filter, require the production overlay, and compare
              the settings keys with the reference table in both directions.
  depends:    —      risk: The stricter gate will start failing on stale
              documentation. That is the intended effect.
  tests:      The checker itself, run over the tree.
  docs:       The list of checks in the module header.
  status:     confirmed (Audit 3 A3-18, A3-25, Audit 5 CFG-07)
```

```text
[F-15] Fifteen of the twenty-six validation rules have no test
  category:   testing
  severity:   Medium   priority: 4
  files:      src/config/validation.rs, src/config/tests.rs
  evidence:   The rules at validation.rs:103, 111, 119, 127, 135, 143, 170,
              188, 196, 204, 212, 239, 263, 271 and 279 have no test.
  impact:     The three most consequential gaps: a `max_length` below the
              `min_length` passes validation and then rejects every password;
              an Argon2 parallelism of zero fails later as an internal error
              during `AppState::new`; a pool size of zero surfaces as a
              runtime database error rather than a start-up refusal.
  fix:        One test per rule.
  depends:    —      risk: None; additive.
  tests:      The tests themselves.
  docs:       docs/testing/strategy.md (the counts).
  status:     confirmed (Audit 5 CFG-08)
```

```text
[F-16] The implementation-status document contradicts itself
  category:   documentation
  severity:   Medium   priority: 7
  files:      docs/engineering/implementation-status.md:40, 74-78
  evidence:   The table marks the configuration as `Verified` and cites 27
              unit tests; the closing paragraph says the configuration module
              "is not covered by a unit test at all".
  impact:     A reader cannot tell whether the configuration is verified.
  fix:        Rewrite the paragraph and reconcile the count.
  depends:    —      risk: None.
  tests:      —      docs: docs/engineering/implementation-status.md.
  status:     confirmed (Audit 2 F-11, Audit 3 A3-03)
```

```text
[F-17] The security architecture understates its own evidence
  category:   documentation
  severity:   Medium   priority: 7
  files:      docs/architecture/security-architecture.md:227
  evidence:   "There is no HTTP-level test of the limiter." The test exists:
              `tests/auth_api.rs:393-420`, and KI-002 is Closed.
  impact:     A security reviewer auditing the limiter would look for a gap
              that does not exist.
  fix:        Update the row.
  depends:    —      risk: None.
  tests:      —      docs: docs/architecture/security-architecture.md.
  status:     confirmed (Audit 2 F-12, Audit 3 A3-14)
```

```text
[F-18] An accepted requirement and an accepted ADR contradict each other
  category:   documentation / architecture
  severity:   Medium   priority: 4
  files:      docs/project-truth.md:279,
              docs/decisions/ADR-0003-auth-token-design.md:202-206,
              docs/database/README.md:154
  evidence:   §12 says "Multi-step writes use an explicit transaction." The
              refresh rotation is two statements with no transaction, and
              ADR-0003 records that the ordering fails safe deliberately.
  impact:     A reviewer cannot tell which rule governs.
  fix:        Amend the requirement to allow the ADR-recorded exception.
  depends:    —      risk: None; documentation only. A transaction would
              change the failure mode the ADR analyses.
  tests:      —      docs: docs/project-truth.md, docs/database/README.md.
  status:     confirmed (Audit 2 F-05, Audit 3 A3-16)
```

```text
[F-19] Three documents describe a CORS `panic!` that no longer exists
  category:   documentation
  severity:   Medium   priority: 7
  files:      docs/architecture/request-lifecycle.md:84-85,
              docs/security/secure-development.md:113-117, 382-386
  evidence:   The code falls back to an empty header value, deliberately, so
              that a mistake can never take the process down.
  impact:     A reviewer following the security rules would hunt for a panic
              that is not there, or restore one from the documentation.
  fix:        Describe the current behaviour and drop the "open finding"
              wording.
  depends:    —      risk: None.
  tests:      —      docs: the three documents.
  status:     confirmed (Audit 2 F-04)
```

```text
[F-20] The operator never sees the readable configuration error
  category:   correctness / operations
  severity:   Medium   priority: 4
  files:      src/main.rs:24, src/config/validation.rs:61-88
  evidence:   `main` returns `Result<(), Box<dyn Error>>`, so Rust's
              `Termination` prints the `Debug` form: the operator sees
              `Error: Missing("DATABASE_URL")` instead of "required
              environment variable DATABASE_URL is missing or empty".
  impact:     The readable wording written for the operator is dead. The
              start-up failure message is the operator's only diagnostic.
  fix:        Print the `Display` form and exit with a non-zero code.
  depends:    —      risk: None.
  tests:      —      docs: docs/architecture/deployment.md.
  status:     confirmed (Audit 2 F-07)
```

```text
[F-21] A migration comment describes a mechanism that is not the one in use
  category:   documentation
  severity:   Medium   priority: 7
  files:      migrations/0001_users_and_sessions.sql:9-11
  evidence:   The comment says the projects→tasks delete "is handled
              explicitly in the service layer"; it is a database cascade
              (`migrations/0003_projects_and_tasks.sql:29`).
  impact:     A reader is sent to the wrong mechanism and to a document that
              does not contain the claim.
  fix:        The migration is applied and must not be edited; record the
              correction in the schema document and the change log.
  depends:    AGENTS.md §4.7 (migrations are append-only)
  risk:       Editing the migration would make the SQLx checksum disagree and
              the next start-up would fail.
  tests:      —      docs: docs/database/schema.md.
  status:     confirmed (Audit 2 F-06)
```

```text
[F-22] The search filters are the only request inputs with no length bound
  category:   validation
  severity:   Medium   priority: 4
  files:      src/notes/dto.rs:95, src/projects/dto.rs:86
  evidence:   Every other request input is bounded and mirrored by a `CHECK`
              constraint. The search term is not bounded anywhere; it reaches
              `plainto_tsquery` and an `ILIKE` pattern.
  impact:     An inconsistency in the validation surface. The term is a bound
              parameter, so this is not an injection, and the scan is over the
              caller's own rows.
  fix:        A bound of 200 characters, enforced in the service.
  depends:    —      risk: A client searching for a long phrase now receives a
              `422`.
  tests:      A unit test for the rule and an integration test for the
              endpoint.
  docs:       docs/api/conventions.md, the two feature API documents.
  status:     confirmed (Audit 2 F-14)
```

### 2.3 Low

The Low findings are listed with the same fields, more compactly. All are
confirmed unless stated otherwise.

| ID | Title | Files | Fix | Risk | Docs |
| -- | ----- | ----- | --- | --- | --- |
| F-23 | The `/api/v1` prefix is written in the route table and in 18 OpenAPI annotations | src/api/mod.rs:88, src/api/routes.rs:172-175, the four `handlers.rs` | One constant, or a contract test that the two lists agree | The published paths must not change | docs/api/ |
| F-24 | The `x-request-id` literal is written twice | src/api/mod.rs:104, 191-193 | Use the existing helper | None | — |
| F-25 | The `Path` and `Query` rejection handlers are the same code | src/api/extract.rs:96-143 | One helper | None | — |
| F-26 | The search-term normalisation is written twice | src/notes/repository.rs:87-91, src/projects/repository.rs:75 | One helper in `utils::text` | None | — |
| F-27 | `AuthService` builds a second `UserRepository` over the same pool | src/auth/service.rs:39-46, 70, 361 | Reach the user through the `UserService` | The 401 and 404 semantics must not change | module-boundaries |
| F-28 | `refresh` repeats the session-creation sequence of `issue_token_pair` | src/auth/service.rs:215-230, 296-318 | One helper | The rotation ordering constraint must stay visible | — |
| F-29 | `AppState::prune_expired_sessions` is a forwarding wrapper with a lossy error conversion | src/state.rs:120-134 | Return the database error directly | Low | — |
| F-30 | `AppState` exposes the services both as a field and through an accessor | src/state.rs:43, 110-113 | One idiom | Public API | module-boundaries |
| F-31 | Three test helpers are dead behind a blanket `#![allow(dead_code)]` | tests/common/mod.rs:12, 243-303 | Use them; remove the blanket | None | testing/strategy |
| F-32 | The `hex` crate is used for exactly one call | Cargo.toml:71, src/auth/token.rs:179 | `format!("{hasher:x}")` | The stored hash encoding must be proven byte-identical | — |
| F-33 | Three dependency features are enabled but never used | Cargo.toml:26, 39, 47 | Remove `axum/macros`, `tokio/sync`, `sqlx/json` | A feature can be needed indirectly; the build verifies | system-overview |
| F-34 | The production overlay sets the page maximum to the value it already had | config/environments/production.toml:29-32 | Delete the no-op block | None | deployment |
| F-35 | The default request timeout is defined three times | src/config/mod.rs:133-138, src/config/settings.rs:369-371, config/app.toml:30 | One definition | None | — |
| F-36 | `AppError::body` repeats the same match arm three times | src/error.rs:228-230 | One or-pattern | None | — |
| F-37 | `authenticate_bearer` builds the same error three times | src/auth/service.rs:356-365 | One local closure | None | — |
| F-38 | Three `chrono` fallbacks restate configured defaults as magic literals | src/auth/service.rs:346-348, src/main.rs:98, src/auth/token.rs:109 | Bound the durations; make the conversion total | The refresh-expiry path | — |
| F-39 | The catalogue holds three byte-identical templates | config/messages.toml:109-115 | One `too_long` template | The rendered text is unchanged | config/README |
| F-40 | `title_too_long` is used for a project *name* | src/projects/service.rs:135 | A correctly named key | None | — |
| F-41 | `AppError::with_message` silently does nothing on a non-validation error | src/error.rs:173-182 | Document the no-op | None | — |
| F-42 | The token tests repeat the access-token lifetime four times | src/auth/token.rs:197, 222, 238, 283 | One constant | None | — |
| F-43 | The unit-test inventory in `testing/strategy.md` has a wrong count, a wrong module count and a double-counted row | docs/testing/strategy.md:37-53 | Recount | None | testing/strategy |
| F-44 | 10.73 MiB of Swagger UI assets are embedded in a binary whose production configuration disables the route | Cargo.toml:89, config/environments/production.toml:27 | Enable the `rust-embed` compression feature, or make the dependency optional | **Measured:** the release binary is 19.35 MB, of which the vendored assets are about half. The binary size was measured on 2026-10-22. | system-overview |
| F-45 | `GET /auth/me` issues the identical users lookup twice | src/api/middleware.rs:60-70, src/auth/service.rs:292 | Serve the profile from the lookup the extractor already made | The 404/403 semantics | — |
| F-46 | The per-request authentication lookup over-fetches and builds a DTO that is thrown away | src/users/repository.rs:91-99, src/auth/service.rs:352-376 | A small projection | The database stays the source of the address | — |
| F-47 | The note-search predicate cannot use the GIN index under a generic plan | src/notes/repository.rs:99, 118, migrations/0002_notes.sql:37 | A static filter fragment | Two SQL variants per method | data-architecture |
| F-48 | The list DTO conversion clones the whole page | src/notes/service.rs:69, src/projects/service.rs:68, src/tasks/service.rs:101 | A by-value conversion | The persistence model is consumed | — |
| F-49 | The list handlers parse the query string twice | src/notes/handlers.rs:37-38 and the two siblings | One combined extractor, two OpenAPI parameter types | The parameters must not be published twice | — |
| F-50 | An empty task list costs an extra round trip | src/tasks/repository.rs:150-164 | Combine the probe and the count | The 404-versus-empty-page distinction | — |
| F-51 | The rate limiter sweeps its whole map on every authentication request | src/ratelimit.rs:63 | Amortise the sweep | The map must stay bounded | deployment |
| F-52 | The token subject crosses the trust boundary as an untyped `String` | src/auth/token.rs:26, 107, src/auth/service.rs:331-333 | Type it as a `Uuid` | The wire format is unchanged | security-architecture |
| F-53 | `validate_settings` is a 190-line function | src/config/validation.rs:102-291 | Split it per settings group | The order of the errors changes | — |
| F-54 | Four feature specifications contradict their own status documents | docs/features/{authentication,projects,tasks}/specification.md | Reconcile them | None | the four feature sets |
| F-55 | `security/README.md` describes the pre-KI-010 rate-limit behaviour | docs/security/README.md:80-82 | Rewrite the item | None | security/ |
| F-56 | `open-questions.md` Q-07 is premised on a defect that was fixed | docs/engineering/open-questions.md:183-186 | Rewrite the context | None | open-questions |
| F-57 | Q-10 says the manifest declares `license = "MIT"` | docs/engineering/open-questions.md:260-262 | Correct the context | None | open-questions |
| F-58 | `module-boundaries.md` has no section for the `helpers` and `utils` modules | docs/architecture/module-boundaries.md | Add two sections | None | module-boundaries |
| F-59 | `conventions.md` says the correlation id appears in the log lines | docs/api/conventions.md:262 | Correct the sentence | None | api/ |
| F-60 | `review-log.md` says no review has taken place, in a file that contains one | docs/engineering/review-log.md:116-118 | Delete the stale sentence | None | review-log |
| F-61 | `system-overview.md` lists `subtle` as a direct dependency | docs/architecture/system-overview.md:156 | Drop the crate from the row | None | system-overview |
| F-62 | The start-up table hides the connect-info wrapper | docs/architecture/system-overview.md:53 | Show the real call | None | system-overview |
| F-63 | The notes specification points at the wrong section of the schema document | docs/features/notes/specification.md:162 | §5 → §7 | None | notes |
| F-64 | `project-truth.md` §9.11 describes a trusted-proxy mechanism that does not exist | docs/project-truth.md:230-231 | Reword and cross-reference Q-07 | None | project-truth |
| F-65 | The authentication status file misreports what project-truth §17 says | docs/features/authentication/status.md:23-26 | Rewrite the paragraph | None | authentication |
| F-66 | The change log records a fix that was not applied | docs/engineering/change-log.md:303 | Apply it or amend the entry | None | change-log |
| F-67 | `testing/strategy.md` §1.5 says the configuration module has no unit test | docs/testing/strategy.md:136-137 | Point at the 26 tests | None | testing/strategy |
| F-68 | `testing/strategy.md` claims twenty security cases and lists eleven | docs/testing/strategy.md:99-114 | Reword or complete | None | testing/strategy |
| F-69 | `testing/strategy.md` names the wrong function for the test-database guard | docs/testing/strategy.md:146-148 | Name `test_database_url()` | None | testing/strategy |
| F-70 | The repository layout in `README.md` and `AGENTS.md` omits two modules | README.md:184-204, AGENTS.md §2 | Add `helpers/` and `utils/` | None | README, AGENTS |
| F-71 | `data-architecture.md` says "Two gaps" and lists three | docs/architecture/data-architecture.md:222 | Count correctly | None | data-architecture |
| F-72 | `deployment.md` calls the body limit a "duration" | docs/architecture/deployment.md:174 | Split the sentence | None | deployment |
| F-73 | The CORS preflight cache lifetime is a magic number | src/api/mod.rs:186 | Name it as a constant | None | — |
| F-74 | The graceful-shutdown drain is a hard-coded 100 ms | src/main.rs:140-141 | A setting, or a named constant | A longer default slows every shutdown | deployment |
| F-75 | The log format is chosen by the build profile, not by `APP_ENV` | src/telemetry.rs:22 | Select it from the environment | A visible but low-impact change | deployment |
| F-76 | `telemetry::init` falls back to `info`, contradicting the configuration contract | src/telemetry.rs:18 | Take a validated filter | A panic instead of a fallback | deployment |
| F-77 | The timeout message is hard-coded next to the catalogue entry | src/api/mod.rs:140 | Use the catalogue | None | — |
| F-78 | Five copies of the product name and a dead OpenAPI `info` block | config/app.toml:15, config/api.toml:8, src/config/settings.rs:344, 362, src/api/routes.rs:22-23 | One source | None | — |
| F-79 | A start-up failure can print a raw driver error | src/main.rs:41-57 | Name the SQLSTATE, not the driver text | Less diagnostic detail | security/ |
| F-80 | The test harness prints the whole connection URL, password included | tests/common/mod.rs:99 | Report the database name only | None | — |
| F-81 | The loader trims and drops empty values twice | src/config/loader.rs:50-55, 100-104 | One normalisation function | None | — |
| F-82 | `Messages::from_toml` and `Config::defaults` are public API with no production caller | src/config/messages.rs:157, src/config/mod.rs:84 | Mark them test-only | The integration harness depends on `defaults()` | — |
| F-83 | The filter predicate is duplicated between the page query and the count query | src/notes/repository.rs:98-99 and :117-118, the two siblings | A window function | The empty-page case returns no total | data-architecture |

**Suspected, not confirmed** — F-83 needs an `EXPLAIN` before it is acted on,
and F-44 needs a release build before the binary size is quoted. Neither is
claimed as a measured improvement here.

---

## 3. Checks that came back clean

These were checked and are correct. They are recorded so that the next reader
does not re-audit them, and so that nobody "fixes" them.

**Security and authorization**

- The authenticated identity is derived only from the credential. No DTO
  carries a settable `user_id`.
- Every private route takes the `AuthUser` extractor; the three public routes
  are the only ones without it, and all three are rate limited.
- An inaccessible resource returns `404`, never `403`. The single `403` for
  another account's session is the documented exception.
- Ownership is enforced in SQL in every repository method, by the caller's
  `user_id` in a bound parameter. The only unscoped reads are the two
  documented identity lookups (the caller's own row, and the session row keyed
  by an unguessable token hash).
- `X-Forwarded-For` is never read.
- No secret reaches a log line, an error body or a `Debug` output.
- The login timing equalisation is real: a dummy Argon2 verification at the
  same cost.
- Every statement is parameterised; no SQL is built by concatenation.

**Async and resources**

- No blocking work on a Tokio worker: Argon2 goes through `spawn_blocking`,
  and the hasher and the dummy hash are built once at start-up.
- No lock is held across an `.await`. The rate limiter's mutex is the only
  one in the crate.
- One `tokio::spawn` in the tree, for the prune task; it is cancelled with
  the runtime.
- The connection pool is created once and shared; `PgPool::clone` is an `Arc`
  clone.
- The rate limiter's map and the refresh-session table are both bounded.
- The configuration is read exactly once, before the socket is bound. No
  `env::var` happens per request.

**Architecture**

- No handler file imports `sqlx`; the 21 handlers contain no SQL.
- No service constructs an HTTP response.
- Exactly one error type on the request path, with one stable code and one
  status per variant, and every code defined exactly once.
- Database and internal failures render a fixed message; only the SQLSTATE
  and the error kind are logged.
- Every extractor rejection becomes the ATOM envelope; the `400`/`422` split
  matches the published contract.
- The middleware stack is assembled in one place and the order matches the
  documentation.
- Every feature module has the same five files and no feature module reaches
  into another's internals.
- Migrations are append-only, embedded, and applied before the bind.
- The shutdown path is sound: a signal is awaited, `with_graceful_shutdown`
  is used, and the request timeout bounds the in-flight work.

**Standards**

- `cargo fmt --all -- --check` and
  `cargo clippy --all-targets --all-features -- -D warnings` are clean.
- No `unwrap`, `expect`, `panic!` or `todo!` in a request path. Every hit is
  in a test module, in the telemetry set-up, or in a default-value helper.
- `PATCH` is partial everywhere: `COALESCE($n, column)` in every partial
  update.

---

## 4. Deliberate duplication — retained by design

These look like duplication and are not. Each was checked against the
decision that requires it.

| ID | What | Why it stays | Authority |
| -- | ---- | ------------ | --------- |
| R-01 | The ownership predicate repeated in every statement | The security property *is* that the predicate is visible in the statement and checkable at a glance. A helper that injected it would hide it from the reviewer. | ADR-0005 |
| R-02 | `Page<T>` next to `NotePage`, `ProjectPage`, `TaskPage` | The envelope is internal, the named schema is the OpenAPI type. utoipa cannot express `Page<Item>` as one named schema per feature. The component registry lists the three named schemas and not `Page`. | `src/pagination.rs`, `docs/api/conventions.md` |
| R-03 | The four `From<&Model> for Response` impls | The models/dto split is the point: the response deliberately omits the password hash and the owner id. A derive would couple the public contract to the column order. | `src/users/models.rs`, `src/users/dto.rs` |
| R-04 | The repeated `AppError::NotFound(messages.resources.X_not_found)` | The wording is defined once, in `config/messages.toml`. The repetition is the call site resolving the catalogue. | ADR-0006 §5 |
| R-05 | The constants that mirror the database `CHECK` constraints | A value the database enforces stays a constant in the code, next to the constraint it mirrors, because a migration is the enforced copy and cannot read a file. | AGENTS.md §4.11 |
| R-06 | The security floors (`MIN_SECRET_BYTES`, `MIN_ARGON2_MEMORY_KIB`) | A floor that is configurable is a floor an operator can lower. | `src/config/validation.rs` |
| R-07 | The SQLSTATE codes | Protocol constants, documented where they are defined. | `src/users/repository.rs` |
| R-08 | The `Bearer` scheme, the header names, the status codes | Protocol constants. The `Bearer` scheme is matched case-insensitively, which is RFC 7235 §2.1. | — |
| R-09 | The repeated OpenAPI response blocks in the annotations | utoipa 5 has no shared-response construct, and the per-endpoint status sets genuinely differ. The repetition buys a generated contract co-located with the handler it describes. | `docs/testing/strategy.md` |
| R-10 | The near-identical handler and service shapes across notes, projects and tasks | The module boundaries forbid a shared CRUD base: notes must not depend on projects or tasks. | `docs/architecture/module-boundaries.md` |
| R-11 | The `code` parameter of `AppError::validation_with` | A deliberate extension point in a published error contract; every current use passes `validation_failed`. | `docs/api/error-contract.md` |

---

## 5. What the audit did not cover

Stated honestly, so that the scope is not overclaimed.

- No load or performance measurement was made. No benchmark exists in the
  repository. Every performance statement in this report is a *reasoned*
  opportunity; the only measured figures are the byte count of the vendored
  Swagger UI assets (10.73 MiB), the size of the release binary (19.35 MB)
  and the package count of the dependency graph.
- No `EXPLAIN` was run against a live database. The index findings (F-47 and
  the two already documented gaps) are read from the migrations and the
  predicates.
- The external integrations were not exercised against a third party: there
  are none in Phase 1 beyond PostgreSQL.
- The threat model was reviewed against the source, not re-derived.
- The audit did not review the deployment of a production instance; the
  production checklist was checked against the code and the configuration
  files.
