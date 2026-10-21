# ATOM — API

**ATOM** is an API-first personal workspace platform: personal notes,
GitHub-style project and task tracking, and — later — automation and
personal knowledge management. This repository is the **Phase 1 backend**.

> The canonical product requirements live in
> [`docs/project-truth.md`](docs/project-truth.md). The documentation index is
> [`docs/README.md`](docs/README.md).

---

## Status

Phase 1 is **in progress**. The backend foundation is implemented and covered
by automated tests, and a CI pipeline runs the quality gates on every change.
The container image is not built yet.
The current, evidence-backed state of every feature is in
[`docs/engineering/implementation-status.md`](docs/engineering/implementation-status.md).

| Area | Status |
| ---- | ------ |
| Authentication, token lifecycle, authorization | Verified |
| Notes, projects, tasks | Verified |
| OpenAPI document and API explorer | Implemented |
| Health endpoints, structured logging | Implemented |
| CI pipeline | Implemented |
| Container image | Not started |
| Everything on the roadmap | Not started, by design |

## Stack

| Concern | Choice |
| ------- | ------ |
| Language | Rust, stable, edition 2024 |
| HTTP | Axum 0.8 on Tokio |
| Database | PostgreSQL, accessed with SQLx 0.8 |
| Migrations | SQLx migrations, embedded in the binary |
| API documentation | OpenAPI 3.1 via `utoipa`, Swagger UI |
| Logging | `tracing` with structured output |
| Password hashing | Argon2id |
| Tokens | HS256 access tokens, opaque hashed refresh tokens |

The decisions behind these choices are recorded as ADRs in
[`docs/decisions/`](docs/decisions/).

## Prerequisites

- **Rust stable** with `rustfmt` and `clippy`
  (`rustup toolchain install stable --component rustfmt,clippy`).
- **PostgreSQL 14 or newer.** A local server, or Docker with the bundled
  compose file.
- Optionally **Docker** to run PostgreSQL without installing it.

## Quick start

### 1. Database

With Docker:

```bash
docker compose up -d postgres
```

Without Docker, using a local PostgreSQL server:

```bash
createdb atom
createdb atom_test
```

### 2. Configuration

```bash
cp .env.example .env          # Linux/macOS
Copy-Item .env.example .env   # Windows PowerShell
```

At minimum, set:

```dotenv
DATABASE_URL=postgres://atom:atom@127.0.0.1:5432/atom
TEST_DATABASE_URL=postgres://atom:atom@127.0.0.1:5432/atom_test
APP_JWT_SECRET=<at least 32 random bytes>
```

Generate a development secret:

```bash
openssl rand -base64 48          # Linux/macOS
```

`.env` is git-ignored. **Never commit it.** `.env.example` holds placeholders
only.

### 3. Run

```bash
cargo run
```

Migrations are applied automatically at start-up. The API listens on
`http://127.0.0.1:3000` by default.

```bash
curl http://127.0.0.1:3000/health/live
# {"status":"live"}

curl http://127.0.0.1:3000/health/ready
# {"status":"ready","checks":{"database":"ok"}}
```

### 4. Explore the API

- OpenAPI document: <http://127.0.0.1:3000/api/v1/openapi.json>
- API explorer: <http://127.0.0.1:3000/api/v1/docs/>

## API at a glance

All endpoints live under `/api/v1` and all of them except registration and
login require `Authorization: Bearer <access_token>`.

```bash
# Register
curl -X POST http://127.0.0.1:3000/api/v1/auth/register \
  -H 'content-type: application/json' \
  -d '{"email":"ada@example.com","password":"Correct-Horse-42","display_name":"Ada"}'

# Login
curl -X POST http://127.0.0.1:3000/api/v1/auth/login \
  -H 'content-type: application/json' \
  -d '{"email":"ada@example.com","password":"Correct-Horse-42"}'

# Create a note
curl -X POST http://127.0.0.1:3000/api/v1/notes \
  -H 'content-type: application/json' \
  -H "authorization: Bearer $TOKEN" \
  -d '{"title":"Release checklist","content":"cut the tag"}'

# Create a project and a task in it
curl -X POST http://127.0.0.1:3000/api/v1/projects \
  -H 'content-type: application/json' \
  -H "authorization: Bearer $TOKEN" \
  -d '{"name":"ATOM Phase 1"}'

curl -X POST http://127.0.0.1:3000/api/v1/projects/$PROJECT_ID/tasks \
  -H 'content-type: application/json' \
  -H "authorization: Bearer $TOKEN" \
  -d '{"title":"Draft the ADR","priority":"high"}'
```

The full contract is in [`docs/api/`](docs/api/).

## Quality gates

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
TEST_DATABASE_URL=postgres://atom:atom@127.0.0.1:5432/atom_test cargo test --all-targets
```

`make check` runs all three. The integration suite refuses to run against a
database whose name does not contain `test`, so it cannot damage a real
database.

## Repository layout

```text
atom/api/
├── src/
│   ├── api/          routing, middleware, extractors, OpenAPI
│   ├── auth/         registration, login, token lifecycle
│   ├── users/        user persistence and the public user shape
│   ├── notes/        personal notes
│   ├── projects/     projects
│   ├── tasks/        tasks
│   ├── bin/atom-docs.rs   documentation checker
│   └── config.rs error.rs state.rs pagination.rs ratelimit.rs telemetry.rs health.rs
├── migrations/       SQLx migrations, embedded in the binary
├── tests/            HTTP-level integration tests
├── docs/             the project knowledge base
├── scripts/          validation helpers
├── .opencode/        the OpenCode agent team, commands and skills
├── AGENTS.md         instructions every agent must read
└── opencode.jsonc    OpenCode project configuration
```

## Working on this project

- **Human contributors:** [`CONTRIBUTING.md`](CONTRIBUTING.md).
- **AI agents:** [`AGENTS.md`](AGENTS.md), then
  [`docs/README.md`](docs/README.md) and
  [`docs/project-truth.md`](docs/project-truth.md). The OpenCode agent team is
  described in [`docs/architecture/`](docs/architecture/) and the workflow in
  [`.opencode/skills/atom-delivery/SKILL.md`](.opencode/skills/atom-delivery/SKILL.md).

Documentation is part of the product. A code change is not complete until its
documentation is updated, or until the author has explained why no
documentation change was necessary.

## License

**The licence has not been chosen yet.** The crate is marked `publish = false`
so it cannot be published to a registry by accident. Before the repository is
released publicly, the maintainer must pick a licence, add the corresponding
`LICENSE` file, and declare it in `Cargo.toml`. See
[`CONTRIBUTING.md`](CONTRIBUTING.md) §9.

## Security

Do not report a vulnerability through a public issue. Use the private security
advisory of the repository. See
[`docs/security/secure-development.md`](docs/security/secure-development.md)
for how to handle secrets while developing.
