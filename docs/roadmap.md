# ATOM roadmap

> **Status:** Proposed (living document; the items whose status column says
> `Accepted` are agreed in [`project-truth.md`](project-truth.md) §18)
> **Last updated:** 2026-10-24
> **Owner:** product owner, maintained by the `docs` agent

The future of ATOM beyond the backend foundation. This document lists the
delivery phases, the features that have been identified, what each one depends
on, and which of them are agreed.

**A roadmap row is not a promise.** An item is `Proposed` until a requirement
is written into [`project-truth.md`](project-truth.md) with acceptance
criteria; from that moment it is `Accepted`, and this document says so. The
Phase 2 items are `Accepted` because §18 of the product requirements records
them. Everything marked `Proposed` here is a note about what has been thought
about, and must not be built as agreed scope.

The current status of the implementation is in
[`engineering/implementation-status.md`](engineering/implementation-status.md).

---

## 1. How to read this document

| Column | Meaning |
| ------ | ------- |
| **Feature** | The name of the capability. |
| **Status** | `Accepted` when the requirement is recorded in [`project-truth.md`](project-truth.md) — §4 for Phase 1, §18 for Phase 2; `Implemented` when the item exists and compiles; `Proposed` otherwise. The seeder and reimbursement tracking are `Implemented` today; nothing in Phase 2 is `Verified`. |
| **Phase** | The delivery phase the item belongs to. The phases are defined in §2. |
| **Depends on** | The capabilities the item needs. |
| **Notes** | The constraint the item must not break, and the thing that makes it non-trivial. |

Two rules follow, and they are the reason the column exists:

1. **`Accepted` means the requirement exists, not that the code does.** The
   status of the implementation is a different question, answered in
   [`project-truth.md`](project-truth.md) §17 and in the feature's `status.md`.
2. **A `Proposed` item may not be built.** If a change needs it, the change
   starts by accepting the requirement, not by adding a hidden implementation.

## 2. The phases

| Phase | Name | Status | What it delivers |
| ----- | ---- | ------ | ---------------- |
| 1 | The backend foundation | `In progress` | The modular monolith, authentication, notes, projects, tasks, the versioned API, the documentation and the test suite. Accepted in [`project-truth.md`](project-truth.md) §4. |
| 2 | The personal-workspace layer | `In progress` | Reimbursement tracking, job application tracking and local machine learning over the owner's notes, on the Phase 1 foundation. Accepted in [`project-truth.md`](project-truth.md) §18; the reimbursements endpoints now exist (all twelve published operations), the seeder exists, and job applications and machine learning do not. Nothing in the phase is `Verified`. |
| 3+ | Unassigned | — | Everything in §3 whose phase is "Later — unassigned". |

### 2.1 Phase 2 — the personal-workspace layer

**This is the answer to `Q-11`.** `Q-11` asked what the phases after Phase 1
are. The answer recorded on 2026-10-24 is that Phase 1 is followed by **one**
named phase, and that anything beyond it stays unnumbered until an item is
accepted.

Phase 2 is the first phase that makes ATOM useful as a personal workspace
rather than only as an API foundation. It contains:

1. **Reimbursement tracking** — claims, line items, a five-state lifecycle and
   a total that is always the sum of the parts
   ([`features/reimbursements/`](features/reimbursements/)).
2. **Job application tracking** — companies, applications, a seven-stage
   machine and a dated timeline of events
   ([`features/job-applications/`](features/job-applications/)).
3. **Local machine learning** — embeddings computed in the API process, and
   lexical, semantic and hybrid note search, with a documented degradation to
   lexical results ([`features/ml/`](features/ml/)).
4. **Hardening of the existing API** — the contract test, the ownership sweep
   and the performance pass that the Phase 1 close-out leaves behind.

Phase 2 also delivers a **database seeder** — one command that produces a
deterministic, complete development dataset. The seeder is development
**tooling**, not product behaviour: it is not a requirement in
[`project-truth.md`](project-truth.md), its specification is
[`database/seeding.md`](database/seeding.md), and it is `Implemented` as
tooling without being adopted as product scope. It is listed in §3 so that a
reader does not mistake it for product scope, and its row records that the
chain's review rows are outstanding.

Three decisions belong to Phase 2 and are recorded as accepted ADRs:

| ADR | Decision |
| --- | -------- |
| [`ADR-0007`](decisions/ADR-0007-money-representation.md) | Money is `NUMERIC(19,4)` in PostgreSQL and a string with four decimals in JSON. |
| [`ADR-0008`](decisions/ADR-0008-local-pure-rust-ml.md) | Inference is local and pure Rust, with Burn and the `flex` backend. |
| [`ADR-0009`](decisions/ADR-0009-receipt-references.md) | A receipt is a reference, not an uploaded file. |

**Phase 2 is complete when** the three features are `Verified` in their
`status.md`, the hardening items are closed, the database schema document
matches the migrations, the seeder seeds every Phase 2 table, the quality gates
pass, and the documentation checker reports no error.

### 2.2 Why there is no Phase 3 yet

Naming a phase is a commitment to its contents. The items in §3 have
dependencies that are not satisfied — a web client needs a stable API, team
membership needs a different ownership model — and none of them has an
accepted requirement. They therefore stay "Later — unassigned" rather than
"Phase 3", which would imply an order and a promise.

## 3. The features

| Feature | Status | Phase | Depends on | Notes |
| ------- | ------ | ----- | ---------- | ----- |
| Reimbursement tracking | `Implemented` | 2 | Projects (optional link) | Requirements are `REIMB-R01`…`R11` in [`project-truth.md`](project-truth.md) §18.1. Single owner, one currency per claim, no FX. A receipt is a reference, not an upload ([`ADR-0009`](decisions/ADR-0009-receipt-references.md)). All twelve published operations exist and their suite passes, but no reviewer has read the chain that completed them, so the feature is not `Verified`. |
| Job application tracking | `Accepted` | 2 | Tasks (optional link) | Requirements are `JOB-R01`…`R09` in [`project-truth.md`](project-truth.md) §18.2. It tracks applications and events; it **does not** generate text and does not scrape a job board. |
| Local machine learning | `Accepted` | 2 | Notes, job applications | Requirements are `ML-R01`…`R09` in [`project-truth.md`](project-truth.md) §18.3. Local and pure Rust ([`ADR-0008`](decisions/ADR-0008-local-pure-rust-ml.md)); no vector-database service; no outbound AI call. |
| Database seeder (development tooling) | `Implemented` | 2 | The Phase 1 schema and every Phase 2 table | Development-only. Not product behaviour and not in [`project-truth.md`](project-truth.md); specified in [`database/seeding.md`](database/seeding.md). `src/bin/atom-seed.rs` exists with 18 passing integration tests and 6 unit tests; the chain's review rows (`D2.S1` rows 10–11) are outstanding. A feature that adds a table without adding it to the seeder is not complete. |
| n8n workflow automation | `Proposed` | Later — unassigned | Projects, tasks, notes | An external integration, and **not Phase 2** (owner decision O7). It is a separate Node service that calls the API; the API never depends on it and it never sits inside the data path of a private record. It does not violate the Rust-only rule because it is not part of the API runtime. |
| Web frontend | `Proposed` | Later — unassigned | The Phase 1 API | A separate codebase and a separate consumer of the API. The API is already CORS-enabled against an explicit origin allowlist for it. The refresh-token and CSRF questions are `Q-02` and `Q-05`. |
| Mobile frontend | `Proposed` | Later — unassigned | The Phase 1 API | A separate codebase. The bearer-token model suits a mobile client without the cookie and CSRF obligations a browser brings. |
| OpenCode prompt and session knowledge management | `Proposed` | Later — unassigned | Notes | Stores prompts and session metadata as notes. It must keep notes independent of projects and tasks, so that attaching a note to a task does not later require a schema migration of the notes table. |
| Labels, comments, activity history, issue numbers | `Proposed` | Later — unassigned | Projects, tasks | Modelled on GitHub, not implemented. Each one widens the ownership model, and a comment or a history entry is another object that a query must scope by owner. |
| Team membership and shared workspaces | `Proposed` | Later — unassigned | Users, projects | The largest change on the list, because it replaces the single-owner assumption with a membership model. It invalidates the ownership predicates that Phase 1 enforces in SQL, so it is an architectural change, not an addition. |

## 4. The dependency order

The items are not independent. The order below is the dependency order from
[`project-truth.md`](project-truth.md) §15 and §15.1, restated for the roadmap.

```text
Phase 1 (accepted)
  users ──► auth ──► notes ─────────────► (later) prompt/session knowledge
                 │         │
                 │         └──────────► (Phase 2) local ML and hybrid search
                 └──► projects ──► tasks
                          │           │
                          │           └──► (later) n8n automation
                          │                (later) labels, comments, history
                          └──► (later) team membership, shared spaces

Phase 2 (accepted)
  projects ──(optional)──► claims (reimbursements)
  tasks    ──(optional)──► applications (job tracking)
  companies ────────────► applications ──► events
  notes, applications ──► embeddings ──► search, match

Phase 1 API ──► (later) web frontend
            └──► (later) mobile frontend
```

Reading of the graph:

1. **The clients depend on nothing but the API.** A web or a mobile frontend
   can be built as soon as the API is stable, which is why they are the
   cheapest items on the list and the most visible.
2. **The knowledge features depend on notes alone.** Prompt and session
   management reads the notes table; local ML reads notes and, from Phase 2,
   job applications. Neither needs projects or tasks.
3. **The automation features depend on the task model.** n8n automation
   reacts to changes in projects and tasks and writes back into the record;
   job tracking uses a task as an optional next action.
4. **Phase 2 adds optional links, never required ones.** A claim without a
   project and an application without a task are both complete.
5. **The collaboration features depend on the ownership model itself.** Team
   membership is not a feature that sits next to the current design; it
   replaces it. It is listed last for that reason.

## 5. The Phase 1 boundary

Phase 1 is the backend foundation. The following are **not** in Phase 1, and
their absence is a decision rather than an oversight.

### 5.1 The features that are Phase 1 scope

The accepted Phase 1 requirements are in
[`project-truth.md`](project-truth.md) §4, and each one has a feature
document under [`features/`](features/). In summary: the modular monolith, the
SQLx persistence with migrations, the typed configuration, the error contract,
the structured logging, the health endpoints, the versioned API, the OpenAPI
document, the documentation, the whole authentication and authorization
boundary, notes, projects and tasks, and the quality requirements.

### 5.2 The explicit non-goals of Phase 1

These are excluded from Phase 1 by
[`project-truth.md`](project-truth.md) §6. Their absence is a decision.

| Excluded from Phase 1 | Note |
| --------------------- | ---- |
| Frontend or mobile user interface | The API is the deliverable of Phase 1. The clients are separate codebases and separate roadmap items. |
| n8n deployment, or any n8n integration | The integration is a later phase; deploying n8n is not ATOM's job at all. |
| OnlineJobs.ph scraping and client-response automation | Not built, and not planned for any phase. |
| LLM providers, AI-generated content, or any outbound AI call | ATOM makes no outbound call to an AI service. §6.1 of the product requirements keeps this exclusion and permits local inference only. |
| Embedding models, a vector database, or retrieval-augmented generation | Excluded from Phase 1. Phase 2 permits **local** embedding models (§18.3); a vector-database service and retrieval-augmented generation remain excluded. |
| Microservices, message brokers, or event-streaming infrastructure | ATOM is a modular monolith ([`ADR-0001`](decisions/ADR-0001-modular-monolith.md)). |
| Billing, subscriptions, and public multi-tenant collaboration | The product is a personal workspace. Team membership is a later candidate, and it is not commercial multi-tenancy. |
| Enterprise abstractions that no current requirement needs | No requirement asks for them. |
| Full-text search over other users' data, or any shared note space | Data is private by default. There is no shared note space, and a shared space would be a different product. |

### 5.3 Explicitly not planned

The following have **no planned phase**. They are not in the roadmap table
above, and nothing in the accepted requirements asks for them. They are listed
so that a reader does not assume they are merely late.

| Not planned | Why |
| ----------- | --- |
| A hosted or paid service dependency | The build and the runtime must not require a hosted or paid service ([`project-truth.md`](project-truth.md) §8). |
| An outbound call to a third-party AI provider | Excluded by [`project-truth.md`](project-truth.md) §6, unchanged by the Phase 2 amendment (§6.1). |
| A vector-database service | The search design is PostgreSQL plus a bounded in-process re-rank ([`ADR-0008`](decisions/ADR-0008-local-pure-rust-ml.md)). |
| A second language runtime in the API | Owner decision O6. Python, Node, Go and an external model server are all excluded. |
| Public or shared workspaces beyond a single owner | The product is a personal workspace. Multi-user collaboration is a different product, and `team membership and shared workspaces` in §3 is already the largest candidate on the list. |
| A public write API for unauthenticated clients | Every endpoint that touches user data requires a credential. |
| Sequential public identifiers | Public identifiers are UUIDs. This is a fixed decision, not a phase. |
| A second, parallel API version | The API is versioned at the root from the start so that a future version can be added without breaking a client. |

## 6. What has to happen before a roadmap item is built

1. **Accept the requirement.** The feature is written into
   [`project-truth.md`](project-truth.md) with a status of `Accepted` and
   acceptance criteria, and its acceptance is recorded as a change in
   [`engineering/change-log.md`](engineering/change-log.md). Until then, the
   item stays `Proposed` here and may not be built as agreed scope.
2. **Write the feature specification.** A directory under
   [`features/`](features/) with the five documents the feature index
   requires: `specification.md`, `architecture.md`, `api.md`, `testing.md`
   and `status.md`.
3. **Decide the architecture, if the change is structural.** A new ADR under
   [`decisions/`](decisions/) records the decision. An item that replaces the
   ownership model — team membership — certainly needs one.
4. **Plan the work.** The item is placed in a phase in this document. Adding a
   phase is a product decision; the numbering is settled for Phase 2.
5. **Implement it under the normal delivery workflow**, with the same quality
   gates, the same documentation protocol and the same review. A roadmap item
   is not a lower standard; it is a later one.

**Phase 2 has completed steps 1 to 3.** Its requirements are in §18, its
feature documents are under [`features/`](features/), and ADR-0007, ADR-0008
and ADR-0009 record the decisions. Step 4 is the phase definition in §2.

## 7. Related

- [`project-truth.md`](project-truth.md) — the accepted requirements, the
  exclusions and the open decisions.
- [`engineering/implementation-status.md`](engineering/implementation-status.md)
  — what exists today.
- [`engineering/open-questions.md`](engineering/open-questions.md) — the
  decisions that would change this document. `Q-11` is answered by §2; the
  tracker entry is updated separately.
- [`engineering/known-issues.md`](engineering/known-issues.md) — the limits of
  what Phase 1 does.
- [`features/`](features/) — the feature documents of the accepted scope.
- [`database/seeding.md`](database/seeding.md) — the seeder specification.
