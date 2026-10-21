# ATOM — repository instructions

This file is loaded automatically by OpenCode (v2 recognises `AGENTS.md`; it does
**not** fall back to `CLAUDE.md`). It applies to every agent working in this
repository.

Read this file before doing anything else. It is short on purpose: shared
conventions live here, and everything else lives under `docs/`.

---

## 1. What this repository is

`atom/api` is the **Phase 1 backend** of ATOM, an API-first personal workspace
platform. It is a Rust modular monolith: Axum + Tokio on top of PostgreSQL via
SQLx.

- The canonical product requirements are in [`docs/project-truth.md`](docs/project-truth.md).
- The documentation index and the authority order are in [`docs/README.md`](docs/README.md).
- The architecture is in [`docs/architecture/`](docs/architecture/).

**Never** treat a roadmap item as agreed scope. Only `docs/project-truth.md`
and accepted ADRs define product behaviour.

---

## 2. Layout

```text
atom/api/
├── Cargo.toml, Cargo.lock, rust-toolchain.toml
├── opencode.jsonc          # OpenCode project configuration (v2 schema)
├── AGENTS.md               # this file
├── CONTRIBUTING.md         # human contributor guide
├── .opencode/
│   ├── agents/             # the agent team
│   ├── commands/           # reusable slash commands
│   └── skills/             # reusable workflows
├── migrations/             # SQLx migrations (source of truth for the schema)
├── scripts/                # validation and helper scripts
├── src/
│   ├── api/                # routing, middleware, extractors
│   ├── auth/               # authentication and token lifecycle
│   ├── users/ notes/ projects/ tasks/
│   ├── config.rs error.rs state.rs pagination.rs ratelimit.rs health.rs
├── tests/                  # HTTP-level integration tests
└── docs/                   # the project knowledge base
```

Every feature module keeps the same internal layering:

| Layer    | Responsibility                                                        |
| -------- | --------------------------------------------------------------------- |
| handlers | HTTP extraction, status codes, response serialization                |
| service  | business rules, validation, transaction coordination                  |
| repository | SQL, parameterised queries, ownership predicates                    |
| models   | persistence shapes                                                   |
| dto      | public request/response contracts                                    |

Keep that layering. Do not let handlers talk to the database directly.

---

## 3. Commands

Run from the repository root.

```bash
# Quality gates — run all three before declaring work done
cargo fmt --all -- --check      # formatting
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets         # unit + integration tests

# Local development
cargo run                        # start the API on 127.0.0.1:3000
cargo sqlx migrate run            # apply migrations (needs a live DATABASE_URL)
```

The integration suite needs a **disposable** database:

```bash
TEST_DATABASE_URL=postgres://atom:atom@127.0.0.1:5432/atom_test cargo test --all-targets
```

The harness refuses to run against a database whose name does not contain
`test`. Never point it at anything else.

Windows PowerShell equivalent:

```powershell
$env:TEST_DATABASE_URL = "postgres://atom:atom@127.0.0.1:5432/atom_test"
cargo test --all-targets
```

---

## 4. Engineering rules

These are not negotiable and are not stylistic preferences.

1. **Never use `.unwrap()` or `.expect()` in a request path.** Use `Result` and
   `?`. `unwrap` is acceptable in tests and in `main` startup checks.
2. **Never build SQL by string concatenation.** Use bound parameters.
   `sqlx::query_as` with `.bind(...)` only. The one exception is a static
   filter fragment that contains no user input.
3. **Every query that touches user data must be scoped by the caller's
   `user_id`.** Ownership is enforced in SQL, not in the handler. A resource
   that the caller does not own must return `404`, never `403`.
4. **Never log or return secrets.** Passwords, tokens, the signing secret and
   the database URL must never appear in a log line, an error message or an
   API response. `AppError` already redacts internal details; keep it that way.
5. **Errors have one shape.** Return `AppResult<T>`; never build a response by
   hand in a handler.
6. **PATCH semantics are partial.** An omitted field is left untouched. Do not
   write `null` over an existing value.
7. **Migrations are append-only.** Never edit an applied migration. Add a new
   one. Every schema change needs a matching model change in the same change.
8. **Do not add a dependency** without a concrete reason recorded in the
   change log.
9. **Do not suppress lints** to make a check pass. Fix the code.
10. **Public contracts are documented.** A new endpoint or a changed response
    shape means updating the OpenAPI annotations, `docs/api/` and the relevant
    feature documents.

---

## 5. Mandatory documentation intake

Documentation is part of the product, not an afterthought. **Every substantial
task follows this protocol.** The full version lives in
[`.opencode/skills/atom-docs/SKILL.md`](.opencode/skills/atom-docs/SKILL.md).

### Before implementation

1. Read this file and any other applicable repository instructions.
2. Read [`docs/README.md`](docs/README.md) — the documentation index.
3. Read [`docs/project-truth.md`](docs/project-truth.md) — the accepted product
   requirements.
4. Read the relevant feature specification under `docs/features/`.
5. Read the related architecture documents and ADRs under `docs/architecture/`
   and `docs/decisions/`.
6. Check `docs/engineering/known-issues.md` and
   `docs/engineering/open-questions.md`.
7. Inspect the real source, migrations, tests and `git diff`.
8. Decide whether the task conflicts with an accepted requirement or an
   unresolved decision. If it does, **stop and say so** instead of silently
   changing product behaviour.

Do not dump every document into context for a small change. Read the canonical
overview first, then retrieve what the task actually needs.

### After implementation

1. Update the feature `status.md` when the status changed.
2. Update the architecture documents when structure, invariants or the request
   lifecycle changed.
3. Add or update an ADR when a decision materially affects structure,
   security, compatibility or long-term maintenance.
4. Append an entry to `docs/engineering/change-log.md`.
5. Record review findings in `docs/engineering/review-log.md`.
6. Update `docs/engineering/implementation-status.md` and
   `docs/engineering/known-issues.md`.
7. Update the OpenAPI annotations and `docs/api/` when a contract changed.
8. Verify that the documentation still matches the code.

**A code change is not complete until its documentation is updated, or you have
explicitly explained why no documentation change was necessary.** Say which one
in your completion report.

---

## 6. Feature status vocabulary

Use exactly these words, and define them the same way everywhere:

| Status       | Meaning                                                                |
| ------------ | ---------------------------------------------------------------------- |
| `Proposed`   | Suggested, not agreed. Must not be implemented as agreed scope.        |
| `Accepted`   | Agreed product requirement, not built yet.                             |
| `In progress`| Being built right now.                                                  |
| `Implemented`| Code exists and compiles. **Not** evidence that it behaves correctly.  |
| `Verified`   | Implemented *and* covered by a test or a recorded check that passes.    |
| `Deprecated` | Accepted earlier, deliberately dropped. The ADR or note says why.      |
| `Blocked`    | Cannot proceed. `docs/engineering/known-issues.md` records the reason. |

Never mark something `Verified` because the code exists or compiles.

---

## 7. Agent behaviour

- The **orchestrator** is the only primary agent. It plans, delegates, integrates
  and verifies. It does not do the implementation work itself.
- Specialists own distinct files. Before two agents edit the same file, agree
  the contract first.
- Every delegated task states: objective, scope and owned files, dependencies
  and contracts, acceptance criteria, expected output, required verification,
  whether the agent may edit, and the prohibited operations.
- **Read-only agents stay read-only.** The code reviewer and the quality gate
  must not modify the implementation they inspect. Their permissions deny the
  `edit` action, deny the `execute` action, and restrict `shell` to an
  allow-list of inspection commands.
- **Never fabricate verification.** If a check could not run, say so and give
  the exact command needed. A truthful "not verified" is required; a false
  "passing" is not acceptable.
- **Never silently fall back to a different model.** If the configured model is
  unavailable or rate-limited, report it and continue with the work that does
  not need the model.
- **Preserve the user's work.** Never run `git reset --hard`, `git clean`,
  `git checkout --` or any history rewrite without an explicit human decision.

---

## 8. Secrets

These files must never be committed, and must never be read into a transcript
or a log:

- `.env` and any `.env.*` file other than `.env.example`
- `*.pem`, `*.key`, `id_rsa*`
- anything under `.ssh/`
- `credentials.json`, `secrets/`
- the OpenCode global configuration in `~/.config/opencode/`

`.env.example` contains placeholders only and is safe to commit.

---

## 9. When you are blocked

Report a blocker instead of asking repeatedly. A blocker report states what you
tried, what failed, the exact error, and what you need. Do not ask for
confirmation for routine decisions — make a reasonable assumption, state it in
the report, and continue.
