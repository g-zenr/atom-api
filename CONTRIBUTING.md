# Contributing to ATOM

Thank you for helping. This guide is the human-facing companion to
[`AGENTS.md`](AGENTS.md), which is the version every automated agent reads.

If you are an agent: read [`AGENTS.md`](AGENTS.md) first, then
[`docs/README.md`](docs/README.md) and [`docs/project-truth.md`](docs/project-truth.md).

---

## 1. Before you change anything

1. Read [`docs/project-truth.md`](docs/project-truth.md). It defines what ATOM
   is and what is explicitly out of scope.
2. Read the feature document for the area you are touching under
   [`docs/features/`](docs/features/).
3. Check [`docs/engineering/known-issues.md`](docs/engineering/known-issues.md)
   and [`docs/engineering/open-questions.md`](docs/engineering/open-questions.md).
4. Run the test suite first, so you know the starting state:

   ```bash
   TEST_DATABASE_URL=postgres://atom:atom@127.0.0.1:5432/atom_test cargo test --all-targets
   ```

If your change contradicts an accepted requirement, **stop and raise it**
rather than quietly changing the product behaviour.

## 2. Setting up

### Prerequisites

- Rust stable with `rustfmt` and `clippy`.
- PostgreSQL 14 or newer, or Docker.

### Database

```bash
docker compose up -d postgres
```

or, with a local server:

```bash
createdb atom
createdb atom_test
```

### Environment

```bash
cp .env.example .env
```

Then set at least `DATABASE_URL`, `TEST_DATABASE_URL` and `APP_JWT_SECRET`
(at least 32 random bytes). `.env` is git-ignored. Never commit it.

### Build and test

```bash
cargo build
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
TEST_DATABASE_URL=... cargo test --all-targets
```

Windows PowerShell:

```powershell
$env:TEST_DATABASE_URL = "postgres://atom:atom@127.0.0.1:5432/atom_test"
cargo test --all-targets
```

## 3. Making a change

1. **Branch.** `feature/<short-description>` or `fix/<short-description>`.
2. **Keep the tree buildable.** One coherent increment at a time.
3. **Follow the engineering rules** in [`AGENTS.md`](AGENTS.md) §4. The ones
   that are most often broken:
   - no `unwrap`/`expect` in a request path,
   - all SQL parameterised,
   - every user-data query scoped by `user_id`,
   - no secret in a log, an error or a response,
   - migrations append-only,
   - PATCH is partial.
4. **Add tests.** A new endpoint needs a success case, an unauthenticated case
   and a cross-account isolation case. A bug fix needs a regression test.
5. **Run the gates.** All three commands above must pass.
6. **Update the documentation.** See §5.
7. **Open a pull request.** Describe what changed, why, and what you verified.

## 4. Database changes

- Migrations are **append-only**. Never edit an applied migration; add a new
  one.
- Every migration needs a matching change in the Rust models.
- Put invariants in the database (`CHECK`, `UNIQUE`, foreign keys) whenever
  practical.
- Index the access paths the API actually uses.
- Apply the migration to a clean database before you push.

The full policy is in
[`docs/database/migration-policy.md`](docs/database/migration-policy.md).

## 5. Documentation is part of the change

A pull request is not complete until its documentation is updated, or until the
author explains in the description why no documentation change was needed.

| You changed | Update |
| ----------- | ------ |
| Behaviour, contract or status of a feature | `docs/features/<feature>/` |
| Structure, invariants or the request lifecycle | `docs/architecture/` |
| A public endpoint, DTO or error code | `docs/api/` and the OpenAPI annotations |
| The schema | `docs/database/schema.md` |
| A security control | `docs/security/` |
| Anything substantial | an entry in `docs/engineering/change-log.md` |

Use the templates in [`docs/templates/`](docs/templates/). The status words are
defined in [`AGENTS.md`](AGENTS.md) §6 — do not invent new ones.

## 6. Using OpenCode

The repository ships a complete OpenCode configuration: an orchestrator and
nine specialists, reusable commands and skills, and a permission model that
keeps routine development autonomous while protecting consequential actions.

```bash
opencode                       # start a session in this directory
opencode run "add note search" # run a one-off task
```

Validate the configuration after changing it:

```bash
./scripts/validate-opencode.sh      # Unix
pwsh ./scripts/validate-opencode.ps1 # Windows PowerShell
```

Check the documentation tree:

```bash
cargo run --bin atom-docs
```

### Changing the model for your own machine

Do **not** edit the committed `opencode.jsonc` for personal preference. Put
your override in `.opencode/opencode.jsonc` — it is git-ignored and takes
precedence over the committed file:

```jsonc
{
  "model": "opencode-go/glm-5.3-flash"
}
```

The default is `opencode-go/minimax-m3.1-flash-free`, a free-tier model that
was verified to work with tool calling and delegation. If your account does
not offer it, pick another model from `opencode models` and put it in your
local override. The full override ladder — including the global
`~/.config/opencode/opencode.jsonc` option and the one-off
`opencode run --model ...` option — is in
[`docs/contributing-opencode.md`](docs/contributing-opencode.md).

## 7. Commit and pull request conventions

- Conventional-Commit prefixes are used for the subject line:
  `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`, `build:`, `ci:`.
- One logical change per commit.
- Write the subject in the imperative mood, under 72 characters.
- Explain *why* in the body when it is not obvious.

## 8. Review checklist

Before you request a review:

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo test --all-targets` passes, with `TEST_DATABASE_URL` set
- [ ] New behaviour has tests, including the authorization case
- [ ] The documentation is updated
- [ ] The change-log entry records the checks that actually ran
- [ ] No secret, absolute path or personal name in the diff

## 9. License

**The license has not been chosen yet.** The crate is marked `publish = false`
so it cannot be published to a registry by accident, and no `LICENSE` file is
committed, because choosing a license is a decision for the maintainer of the
project rather than something a template should decide silently.

Before the repository is published:

1. Choose a license and add the corresponding `LICENSE` file.
2. Declare it in `Cargo.toml` (`license = "SPDX-ID"`).
3. Note the decision in `docs/decisions/` as an ADR.

Until then, treat the code as "all rights reserved by the project owner" and
do not redistribute it.

## 10. Code of conduct

Be respectful and assume good faith. Review the code, not the person. Security
issues go through the private advisory channel, not a public issue.
