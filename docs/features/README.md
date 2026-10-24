# ATOM features

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent

This directory holds the per-feature documentation. A *feature* is a coherent
slice of product behaviour that a user can observe: a set of endpoints, the
rules behind them, and the data they own.

Every feature directory contains the same five documents. The contract for
each of them is in [§2](#2-the-per-feature-document-contract).

---

## 1. The feature index

| Feature | Identifier | Status | Documents |
| ------- | ---------- | ------ | --------- |
| Authentication | `ATOM-FEAT-AUTH-001` | Verified | [specification](authentication/specification.md) · [architecture](authentication/architecture.md) · [api](authentication/api.md) · [testing](authentication/testing.md) · [status](authentication/status.md) |
| Notes | `ATOM-FEAT-NOTE-001` | Verified | [specification](notes/specification.md) · [architecture](notes/architecture.md) · [api](notes/api.md) · [testing](notes/testing.md) · [status](notes/status.md) |
| Projects | `ATOM-FEAT-PROJ-001` | Verified | [specification](projects/specification.md) · [architecture](projects/architecture.md) · [api](projects/api.md) · [testing](projects/testing.md) · [status](projects/status.md) |
| Tasks | `ATOM-FEAT-TASK-001` | Verified | [specification](tasks/specification.md) · [architecture](tasks/architecture.md) · [api](tasks/api.md) · [testing](tasks/testing.md) · [status](tasks/status.md) |
| Reimbursements | `ATOM-FEAT-REIMB-001` | Implemented | [specification](reimbursements/specification.md) · [architecture](reimbursements/architecture.md) · [api](reimbursements/api.md) · [testing](reimbursements/testing.md) · [status](reimbursements/status.md) |
| Job application tracking | `ATOM-FEAT-JOB-001` | Accepted | [specification](job-applications/specification.md) · [architecture](job-applications/architecture.md) · [api](job-applications/api.md) · [testing](job-applications/testing.md) · [status](job-applications/status.md) |
| Local machine learning | `ATOM-FEAT-ML-001` | Accepted | [specification](ml/specification.md) · [architecture](ml/architecture.md) · [api](ml/api.md) · [testing](ml/testing.md) · [status](ml/status.md) |

The three Phase 2 features were accepted in
[`project-truth.md`](../project-truth.md) §18 with acceptance criteria.
Reimbursements is `Implemented`: `migrations/0004_expense_claims.sql`,
`src/reimbursements/` and all twelve published endpoints exist, and
`tests/claims_schema.rs`, `tests/claims_api.rs`, `tests/claims_flow.rs` and
`tests/openapi_contract.rs` are written against them. Job applications has its
**schema only**: `migrations/0005_job_applications.sql` and
`src/job_applications/{mod,models}.rs`, with `tests/jobs_schema.rs` and `tests/job_applications_api.rs` proving the
constraints and all fourteen published operations. Local machine learning has nothing
yet. Neither is `Implemented` or `Verified`; a passing suite is not a review.

The status column summarises the feature as a whole. The per-requirement
evidence behind it is in each feature's `status.md`; a feature can contain both
verified and merely implemented requirements.

The **Users** capability is not a separate feature directory. User
persistence, normalisation and the public user representation are part of the
[authentication](authentication/specification.md) feature, because in Phase 1
the only way to obtain a user is to register or to authenticate.

Cross-cutting contracts live outside this directory:

| Concern | Document |
| ------- | -------- |
| The HTTP contract as a whole | [API conventions](../api/conventions.md) and the [error contract](../api/error-contract.md) |
| The persistence model | [Database schema](../database/schema.md) |
| The agreed product requirements | [project-truth.md](../project-truth.md) |

## 2. The per-feature document contract

| File | Contains | Never contains |
| ---- | -------- | -------------- |
| `specification.md` | Feature identifier and name, purpose and the user problem it solves, status, scope and explicit non-goals, the accepted requirements, user-facing behaviour, dependencies on other features, acceptance criteria, known limitations, changelog references. | Implementation detail, SQL, or a requirement that is not in [`project-truth.md`](../project-truth.md). |
| `architecture.md` | How the feature is built: responsibilities, interfaces, data ownership, invariants, error handling, the security boundary, the path from handler to service to repository to SQL, the related files, the related ADRs, and the limitations of the design. | A restatement of the public contract (that lives in `api.md`). |
| `api.md` | Every endpoint of the feature: method, path, authentication requirement, request shape, response shape, status codes, error cases, and a `curl` example for the important ones. | Anything the code does not implement. |
| `testing.md` | What is covered, by which test file and which test name, and what is deliberately not covered and why. | A claim that a test was run. The documentation describes the suite; the [verification policy](../testing/verification-policy.md) records what was actually executed. |
| `status.md` | The status line, the evidence for the status, what is still missing to reach `Verified`, and links to the other four documents. | A status outside the vocabulary below. |

A feature is documented in the same five files even when it is small. The
files are short when the feature is small; they are never merged.

## 3. The status vocabulary

Every feature carries exactly one status. The words and their meanings are
defined in [`AGENTS.md`](../../AGENTS.md) §6 and are identical everywhere in
the documentation tree.

| Status | Meaning |
| ------ | ------- |
| **Proposed** | Suggested, not agreed. Must not be implemented as agreed scope. |
| **Accepted** | An agreed product requirement. Not built yet. |
| **In progress** | Being built right now. |
| **Implemented** | The code exists and compiles. **Not** evidence that it behaves correctly. |
| **Verified** | Implemented *and* covered by a test or a recorded check that passes. |
| **Deprecated** | Accepted earlier and deliberately dropped. The ADR or the note says why. |
| **Blocked** | Cannot proceed. The reason is recorded in [`known-issues.md`](../engineering/known-issues.md). |

A feature is never `Verified` because its code exists or compiles.

## 4. How a feature moves between statuses

```text
Proposed ──accepted──► Accepted ──started──► In progress ──code lands──► Implemented
                          ▲                       │                        │
                          │                       └────paused───────────────┘
                          │                                                │
                          │                                          covered by a test
                          │                                                ▼
                          └──────────── superseded ────────────────── Verified
                                                                           │
                                                                           ▼
                                                    Deprecated  ◄──────┘
```

The transitions and what each one requires:

| From | To | What must be true |
| ---- | -- | ------------------ |
| `Proposed` | `Accepted` | The requirement is recorded in [`project-truth.md`](../project-truth.md), or an accepted ADR requires it. |
| `Accepted` | `In progress` | Work has started. The specification lists the requirements. |
| `In progress` | `Implemented` | The code exists, the quality gates pass, and the public contract is documented. |
| `Implemented` | `Verified` | Every accepted requirement of the feature is covered by a named test or by a recorded check whose result is written down. |
| `Implemented` / `Verified` | `In progress` | A change is in flight that will invalidate the current evidence. |
| any | `Deprecated` | The feature was accepted earlier and deliberately dropped. The reason is in an ADR or in the feature's `status.md`. |
| any | `Blocked` | The work cannot proceed. The reason and the unblocking condition are recorded in [`known-issues.md`](../engineering/known-issues.md). |

Two rules make the statuses meaningful:

1. **A status is a claim about evidence, not about intent.** Moving to
   `Verified` requires a named test or a written record of a check. Code that
   merely compiles stays at `Implemented`.
2. **A status is never raised to cover a gap.** If one accepted requirement of
   a feature is implemented but untested, the feature status is `Implemented`,
   and the gap is named explicitly in the feature's `status.md`.

## 5. Conventions used in these documents

- **Feature identifiers** follow `ATOM-FEAT-<AREA>-<NNN>`, for example
  `ATOM-FEAT-AUTH-001`. They are stable for the life of the feature and are
  never reused.
- **Requirement identifiers** are local to the feature and follow
  `<FEATURE-PREFIX>-R<NNN>`, for example `AUTH-R04`.
- **Links** are relative and never point at a line number.
- **Dates and timestamps** are UTC. Prose uses `YYYY-MM-DD`; wire timestamps
  use RFC 3339.
