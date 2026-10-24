# ATOM documentation

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-25
> **Owner:** maintained by the `docs` agent

This directory is ATOM's knowledge base. It is written so that a contributor
who has never had access to a private conversation can work on the project
correctly, and so that an agent can pick up where the previous one stopped.

Start with [`project-truth.md`](project-truth.md). It is the canonical product
requirements document.

---

## 1. The authority order

Sources of truth are **ranked**. When two sources disagree, the higher one
wins for the question it is authoritative about.

| # | Source | Authoritative for | Not authoritative for |
| - | ------ | ------------------ | --------------------- |
| 1 | [`project-truth.md`](project-truth.md) and accepted ADRs in [`decisions/`](decisions/) | the agreed product requirements and the accepted technical decisions | whether the code implements them |
| 2 | [`architecture/`](architecture/), [`api/`](api/), [`database/`](database/), [`security/`](security/) | the current design, the public contract, the schema, the security model | the product scope |
| 3 | [`features/`](features/) | per-feature requirements, contracts, acceptance criteria and status | cross-feature architecture |
| 4 | `src/`, `migrations/`, `tests/` | what the system actually does right now | what it *should* do |
| 5 | [`engineering/`](engineering/) | the historical record: changes, reviews, known issues, open questions | current behaviour |
| 6 | [`roadmap.md`](roadmap.md), and anything marked `Proposed` | ideas and suggestions | anything at all, until accepted |

Three consequences follow, and they matter:

- **A higher-level requirement does not prove a feature is implemented.**
  `project-truth.md` says the notes feature is Accepted; that says nothing
  about whether the code exists.
- **Source code does not change the approved product requirements.** If the
  code and the requirements disagree, that is a discrepancy to record, not a
  licence to rewrite the requirements.
- **The historical record is not the specification.** The change log explains
  *how* the system reached its current state; it does not define the target
  state.

### When sources conflict

1. Name both conflicting sources and quote them.
2. Explain the impact.
3. Check whether an accepted ADR already resolves it.
4. If it is unresolved, record it in
   [`engineering/known-issues.md`](engineering/known-issues.md) (for a defect
   or a limitation) or [`engineering/open-questions.md`](engineering/open-questions.md)
   (for a product decision).
5. Continue with the work that does not depend on the answer.
6. Ask a human only when a material decision genuinely cannot be made safely.

## 2. The status vocabulary

Every requirement and every feature carries exactly one of these statuses. The
definitions are the same everywhere, including in `AGENTS.md`.

| Status | Meaning |
| ------ | ------- |
| **Proposed** | Suggested, not agreed. Must not be implemented as agreed scope. |
| **Accepted** | An agreed product requirement. Not built yet. |
| **In progress** | Being built right now. |
| **Implemented** | The code exists and compiles. **Not** evidence that it behaves correctly. |
| **Verified** | Implemented *and* covered by a test or a recorded check that passes. |
| **Deprecated** | Accepted earlier and deliberately dropped. The ADR or the note says why. |
| **Blocked** | Cannot proceed. The reason is recorded in [`engineering/known-issues.md`](engineering/known-issues.md). |

A feature is never `Verified` because its code exists or compiles.

## 3. Index

### Product and process

| Document | Contents |
| -------- | -------- |
| [`project-truth.md`](project-truth.md) | The canonical product requirements: vision, scope, exclusions, constraints, terminology, dependencies, open decisions, current status. |
| [`roadmap.md`](roadmap.md) | Future features, their status and their dependency order. |
| [`contributing-opencode.md`](contributing-opencode.md) | Driving the ATOM backend with OpenCode: the agent team, the permission model, how to override the model, and the verified behaviour of the installed OpenCode version. |

### Architecture

| Document | Contents |
| -------- | -------- |
| [`architecture/README.md`](architecture/README.md) | Index and the rules for architecture documentation. |
| [`architecture/system-overview.md`](architecture/system-overview.md) | The system in one page: components, runtime shape, deployment topology. |
| [`architecture/module-boundaries.md`](architecture/module-boundaries.md) | Every module that exists, including the three built Phase 2 modules (`reimbursements`, `job_applications` and `ml`): responsibility, public interface, dependencies, data ownership. |
| [`architecture/request-lifecycle.md`](architecture/request-lifecycle.md) | From socket to SQL and back, including middleware order and error mapping. |
| [`architecture/data-architecture.md`](architecture/data-architecture.md) | The schema, the ownership model, search and indexing. |
| [`architecture/security-architecture.md`](architecture/security-architecture.md) | Trust boundaries, the authentication boundary, the authorization model. |
| [`architecture/deployment.md`](architecture/deployment.md) | Configuration, secrets, running it, scaling and the production checklist. |
| [`../config/README.md`](../config/README.md) | The configuration reference: the layers, the precedence, every setting, the secrets, the message catalogue and the validation rules. |

### Decisions

| Document | Contents |
| -------- | -------- |
| [`decisions/README.md`](decisions/README.md) | The ADR process, the statuses and the index. |
| [`decisions/ADR-0001-modular-monolith.md`](decisions/ADR-0001-modular-monolith.md) | ATOM is a modular monolith. |
| [`decisions/ADR-0002-rust-axum-postgresql.md`](decisions/ADR-0002-rust-axum-postgresql.md) | The technology stack. |
| [`decisions/ADR-0003-auth-token-design.md`](decisions/ADR-0003-auth-token-design.md) | The access/refresh token design. |
| [`decisions/ADR-0004-documentation-as-project-truth.md`](decisions/ADR-0004-documentation-as-project-truth.md) | The documentation tree is part of the product. Superseded, in the narrow sense, by ADR-0006. |
| [`decisions/ADR-0005-ownership-enforced-in-sql.md`](decisions/ADR-0005-ownership-enforced-in-sql.md) | Authorization is enforced in SQL, not in handlers. |
| [`decisions/ADR-0006-configuration-source-of-truth.md`](decisions/ADR-0006-configuration-source-of-truth.md) | The configuration directory is the source of truth; the secrets are held only in the environment. |
| [`decisions/ADR-0007-money-representation.md`](decisions/ADR-0007-money-representation.md) | Money is `NUMERIC(19,4)` in the database and a four-decimal string on the wire. |
| [`decisions/ADR-0008-local-pure-rust-ml.md`](decisions/ADR-0008-local-pure-rust-ml.md) | Local, pure-Rust inference with Burn and the `flex` backend; `REAL[]` and a bounded hybrid re-rank instead of pgvector. |
| [`decisions/ADR-0009-receipt-references.md`](decisions/ADR-0009-receipt-references.md) | A receipt is a reference, not an uploaded file, in Phase 2. |

### Features

| Document | Contents |
| -------- | -------- |
| [`features/README.md`](features/README.md) | The feature index and the per-feature document contract. |
| [`features/authentication/`](features/authentication/) | Registration, login, the token lifecycle, `/auth/me`. |
| [`features/notes/`](features/notes/) | Personal notes, archiving, search, pagination. |
| [`features/projects/`](features/projects/) | Projects, their lifecycle, and what happens to their tasks. |
| [`features/tasks/`](features/tasks/) | Tasks, status, priority, ordering, filtering. |
| [`features/reimbursements/`](features/reimbursements/) | Expense claims, line items, money and the claim lifecycle. `Implemented`: all twelve published operations, the schema and the tests exist; `Verified` waits on the chain reviews. |
| [`features/job-applications/`](features/job-applications/) | Companies, applications, stages and events. `Implemented`: migration `0005`, the six-module `src/job_applications/` and the fourteen operations exist; `Verified` waits on a re-review and a clean re-gate after the review's fixes. |
| [`features/ml/`](features/ml/) | Local embeddings, hybrid search and match ranking. `Implemented`: the three operations, the schema and the suite exist; the suite is recorded green in `ATOM-CHANGE-20261024-011`, and `ML-R02` is not met, so nothing is `Verified`. |

Each feature directory contains the same five documents:

| File | Contains |
| ---- | -------- |
| `specification.md` | Purpose, scope, non-goals, accepted requirements, user-facing behaviour, acceptance criteria. |
| `architecture.md` | How the feature is built: interfaces, invariants, data, security boundary. |
| `api.md` | The endpoints, request and response contracts, error cases. |
| `testing.md` | What is covered, by which test, and what is not covered. |
| `status.md` | The current status, with the evidence for it. |

### Contracts, data and security

| Document | Contents |
| -------- | -------- |
| [`api/README.md`](api/README.md) | The API index. |
| [`api/conventions.md`](api/conventions.md) | Versioning, naming, status codes, pagination, timestamps, identifiers, PATCH semantics. |
| [`api/error-contract.md`](api/error-contract.md) | The single error shape, every error code, and the mapping to HTTP status. |
| [`database/README.md`](database/README.md) | The database index. |
| [`database/schema.md`](database/schema.md) | Every table, column, constraint and index. |
| [`database/migration-policy.md`](database/migration-policy.md) | How migrations are written, applied and reviewed. |
| [`security/README.md`](security/README.md) | The security index. |
| [`security/threat-model.md`](security/threat-model.md) | Assets, actors, threats, controls, accepted risks. |
| [`security/secure-development.md`](security/secure-development.md) | The rules a change must follow, and how to check them. |

### Testing

| Document | Contents |
| -------- | -------- |
| [`testing/strategy.md`](testing/strategy.md) | The layers, what each proves, and how the suite is isolated. |
| [`testing/verification-policy.md`](testing/verification-policy.md) | What must pass before a change is complete, and how to report it. |

### The engineering record

| Document | Contents |
| -------- | -------- |
| [`engineering/change-log.md`](engineering/change-log.md) | Newest-first chronological record of substantial changes. Append-only. |
| [`engineering/implementation-status.md`](engineering/implementation-status.md) | The current verified state of every feature. |
| [`engineering/review-log.md`](engineering/review-log.md) | Substantive reviews: findings, required fixes, resolution, re-review evidence. |
| [`engineering/git-workflow.md`](engineering/git-workflow.md) | The branch model, the branch and commit naming rules, the six gates, the pull-request process, and how a review is conducted. |
| [`engineering/release-and-promotion.md`](engineering/release-and-promotion.md) | How a change moves through DEV, UAT and PROD: the gate at each stage, what is verified, what is recorded, and what infrastructure does not exist yet. |
| [`engineering/runs/chain/`](engineering/runs/chain/) | The Phase 2 run: the plan, the self-healing daily prompt and the live ledger for reimbursements, job applications, local ML and the database seeder. |
| [`engineering/known-issues.md`](engineering/known-issues.md) | Confirmed defects, limitations, workarounds and remediation status. |
| [`engineering/open-questions.md`](engineering/open-questions.md) | Unresolved product or architecture decisions. |

### The audits

| Document | Contents |
| -------- | -------- |
| [`audit/audit-report.md`](audit/audit-report.md) | The findings of the repository-wide audits: the evidence, the severity, the fix and the regression risk of each. |
| [`audit/remediation-backlog.md`](audit/remediation-backlog.md) | The remediation tasks, their acceptance criteria and their status. |

The audits are a **record of a review**, not a source of truth about the
product. Where an audit finding and the code disagree, the code is right and
the finding is a report of what the reviewer saw. Where an audit finding and
[`project-truth.md`](project-truth.md) or an accepted ADR disagree, the
requirement wins and the disagreement is recorded.

### Templates

| Document | Use |
| -------- | --- |
| [`templates/feature-specification.md`](templates/feature-specification.md) | A new feature specification. |
| [`templates/adr.md`](templates/adr.md) | A new architecture decision record. |
| [`templates/change-record.md`](templates/change-record.md) | A change-log entry. |
| [`templates/review-record.md`](templates/review-record.md) | A review-log entry. |
| [`templates/verification-record.md`](templates/verification-record.md) | A verification record. |

## 4. Conventions

- **Identifiers.** Changes are `ATOM-CHANGE-YYYYMMDD-NNN`. Reviews are
  `ATOM-REVIEW-YYYYMMDD-NNN`. Verifications are `ATOM-VERIFY-YYYYMMDD-NNN`.
  ADRs are `ADR-NNNN-short-slug`. An identifier is never reused.
- **Dates.** UTC, `YYYY-MM-DD`, with a time when the order matters.
- **Links.** Stable relative links only. Never link to a line number.
- **Timestamps in prose.** Use the ISO 8601 form.
- **Headers.** Every document starts with a status, a last-updated date and
  an owner role.

## 5. Keeping the documentation honest

```bash
cargo run --bin atom-docs            # check the tree
./scripts/check-docs.sh              # the same check, Unix
pwsh ./scripts/check-docs.ps1        # the same check, Windows
```

The checker verifies that:

1. every required document exists;
2. every relative link resolves;
3. every feature status uses the defined vocabulary;
4. every ADR has a status and a unique identifier;
5. every change-log identifier is unique;
6. every feature directory has its five documents;
7. the configuration directory has its required files;
8. `.env.example` and the configuration loader agree about the environment
   variables, in both directions;
9. the configuration files and **both** operator references — the one in
   [`architecture/deployment.md`](architecture/deployment.md) and the one in
   [`../config/README.md`](../config/README.md) — agree about the **names** of
   the settings, in both directions, and about their **integer and boolean**
   defaults.

The value comparison is deliberately narrow, and the code comment says why. Only
a plain TOML integer or boolean is compared: a cell that holds prose (`1 MiB`,
`20 (default)`, `At least 12 characters.`) is skipped rather than reported, and
a key that appears on more than one row of the same document is not compared for
value there, because the repeated rows describe the overlay, floor and
environment layers with a different layer's value. `config/README.md` documents
each setting once, so the default drift this check exists for is caught there.
Name coverage is not narrowed by any of this.

The checker cannot tell you whether a sentence is *true*. That is the job of
the `code-reviewer` and the `quality-gate` agents, and of every human
reviewing a pull request.
