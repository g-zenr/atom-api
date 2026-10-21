# ATOM roadmap

> **Status:** Proposed (living document)
> **Last updated:** 2026-10-21
> **Owner:** product owner, maintained by the `docs` agent

The future of ATOM, beyond the backend foundation. This document lists the
features that have been identified but not agreed, what each one depends on,
and where the Phase 1 boundary is.

**Nothing in this document is agreed scope.** Every item here is `Proposed`.
Implementing one of them requires it to be **accepted** first: the accepted
requirement is written into
[`project-truth.md`](project-truth.md) §4, and the decision, if it is
structural, is recorded as an ADR under [`decisions/`](decisions/). A roadmap
item is not a promise; it is a note about what has been thought about.

The current status of the implementation is in
[`engineering/implementation-status.md`](engineering/implementation-status.md).

---

## 1. How to read this document

| Column | Meaning |
| ------ | ------- |
| **Feature** | The name of the candidate feature. |
| **Status** | `Proposed` for every item here. `Accepted` means the requirement is in [`project-truth.md`](project-truth.md) §4 and the item may be planned. |
| **Phase** | The delivery phase the item belongs to. ATOM defines one phase today — Phase 1, the backend foundation — so everything listed here is in a later, unassigned phase. The phase numbering is `Q-11` in [`engineering/open-questions.md`](engineering/open-questions.md). |
| **Depends on** | The accepted Phase 1 capabilities the item needs. |
| **Notes** | The constraint that the item must not break, and the thing that makes it non-trivial. |

## 2. The candidate features

| Feature | Status | Phase | Depends on | Notes |
| ------- | ------ | ----- | ----------- | ----- |
| n8n workflow automation | `Proposed` | Later — unassigned | Projects, tasks, notes | An external integration. It must not weaken ownership checks, token handling or secret hygiene, and it must not put a third party inside the data path of a private record. The n8n deployment itself is not part of ATOM. |
| Job application and client-response tracking | `Proposed` | Later — unassigned | Projects, tasks | Reuses the task model rather than introducing a new one. The data it holds is a person's job history, which raises the sensitivity of the store: the same ownership rules apply, with less tolerance for a leak. |
| Web frontend | `Proposed` | Later — unassigned | The Phase 1 API | A separate codebase and a separate consumer of the API. The API is already CORS-enabled against an explicit origin allowlist for it. The refresh-token and CSRF questions are `Q-02` and `Q-05`. |
| Mobile frontend | `Proposed` | Later — unassigned | The Phase 1 API | A separate codebase. The bearer-token model in the API fits a mobile client without the cookie and CSRF obligations a browser brings. |
| OpenCode prompt and session knowledge management | `Proposed` | Later — unassigned | Notes | Stores prompts and session metadata as notes. It must keep notes independent of projects and tasks, so that attaching a note to a task does not later require a schema migration of the notes table. |
| Local machine learning and RAG | `Proposed` | Later — unassigned | Notes | Local-first: the model and the index run on the owner's own machine, and no content leaves it. There is no vector database service in Phase 1, and no outbound call to an AI provider. |
| Labels, comments, activity history, issue numbers | `Proposed` | Later — unassigned | Projects, tasks | Modelled on GitHub, not implemented yet. Each one widens the ownership model, and a comment or a history entry is another object that a query must scope by owner. |
| Team membership and shared workspaces | `Proposed` | Later — unassigned | Users, projects | The largest change on the list, because it replaces the single-owner assumption with a membership model. It invalidates the ownership predicates that Phase 1 enforces in SQL, so it is an architectural change, not an addition. |

## 3. The dependency order

The items are not independent. The order below is the dependency order from
[`project-truth.md`](project-truth.md) §15, restated for the roadmap.

```text
Phase 1 (accepted)
  users ──► auth ──► notes ─────────────► (future) prompt/session knowledge
                 │         │
                 │         └──────────────► (future) local ML and RAG
                 └──► projects ──► tasks ──► (future) n8n automation
                                │                (future) job tracking
                                └──► (future) labels, comments, history
                                └──► (future) team membership, shared spaces

  Phase 1 API ──► (future) web frontend
              └──► (future) mobile frontend
```

Reading of the graph:

1. **The clients depend on nothing but the API.** A web or a mobile frontend
   can be built as soon as the API is stable. They are the cheapest items on
   the list and the most visible.
2. **The knowledge features depend on notes alone.** Prompt and session
   management, and local machine learning and RAG, both read the notes table
   and nothing else. They do not require projects or tasks.
3. **The automation features depend on the task model.** n8n automation and
   job tracking both react to changes in projects and tasks, and both write
   back into the record. They share the dependency in
   [`project-truth.md`](project-truth.md) §15.
4. **The collaboration features depend on the ownership model itself.** Team
   membership is not a feature that sits next to the current design; it
   replaces it. It is listed last for that reason.

## 4. The Phase 1 boundary

Phase 1 is the backend foundation. The following are **not** in Phase 1, and
their absence is a decision rather than an oversight.

### 4.1 The features that are Phase 1 scope

The accepted Phase 1 requirements are in
[`project-truth.md`](project-truth.md) §4, and each one has a feature
document under [`features/`](features/). In summary: the modular monolith, the
SQLx persistence with migrations, the typed configuration, the error contract,
the structured logging, the health endpoints, the versioned API, the OpenAPI
document, the documentation, the whole authentication and authorization
boundary, notes, projects and tasks, and the quality requirements.

### 4.2 The explicit non-goals of Phase 1

These are excluded from Phase 1 by
[`project-truth.md`](project-truth.md) §6. Their absence is a decision.

| Excluded from Phase 1 | Note |
| --------------------- | ---- |
| Frontend or mobile user interface | The API is the deliverable of Phase 1. The clients are separate codebases and separate roadmap items. |
| n8n deployment, or any n8n integration | The integration is a later phase; deploying n8n is not ATOM's job at all. |
| OnlineJobs.ph scraping and client-response automation | Not built, and not planned for any phase. |
| LLM providers, AI-generated content, or any outbound AI call | ATOM makes no outbound call to an AI service. The local machine-learning item in the roadmap is local-first for exactly this reason. |
| Embedding models, a vector database, or retrieval-augmented generation | Not in Phase 1. A local RAG approach is a later candidate and would still be local. |
| Microservices, message brokers, or event-streaming infrastructure | ATOM is a modular monolith ([`ADR-0001`](decisions/ADR-0001-modular-monolith.md)). |
| Billing, subscriptions, and public multi-tenant collaboration | The product is a personal workspace. Team membership is a later candidate, and it is not a commercial multi-tenancy. |
| Enterprise abstractions that no current requirement needs | No requirement asks for them. |
| Full-text search over other users' data, or any shared note space | Data is private by default. There is no shared note space in Phase 1, and a shared space would be a different product. |

### 4.3 Explicitly not planned

The following have **no planned phase**. They are not in the roadmap table
above, and nothing in the accepted requirements asks for them. They are listed
so that a reader does not assume they are merely late.

| Not planned | Why |
| ----------- | --- |
| A hosted or paid service dependency | The build and the runtime must not require a hosted or paid service. |
| An outbound call to a third-party AI provider | Excluded by [`project-truth.md`](project-truth.md) §6. A future requirement would have to be accepted, and would have to show that it does not compromise the security of the private record. |
| Public or shared workspaces beyond a single owner | The product is a personal workspace. Multi-user collaboration is a different product, and `team membership and shared workspaces` in §2 is already the largest candidate on the list. |
| A public write API for unauthenticated clients | Every endpoint that touches user data requires a credential. |
| Sequential public identifiers | Public identifiers are UUIDs. This is a fixed decision, not a phase. |
| A second, parallel API version | The API is versioned at the root from the start so that a future version can be added without breaking a client. |

## 5. What has to happen before a roadmap item is built

1. **Accept the requirement.** The feature is written into
   [`project-truth.md`](project-truth.md) §4 with a status of `Accepted`, and
   its acceptance is recorded as a change in
   [`engineering/change-log.md`](engineering/change-log.md). Until then, the
   item stays `Proposed` here and may not be built as agreed scope.
2. **Write the feature specification.** A copy of
   [`templates/feature-specification.md`](templates/feature-specification.md)
   goes into a new directory under [`features/`](features/), with the five
   documents the feature index requires.
3. **Decide the architecture, if the change is structural.** A new ADR under
   [`decisions/`](decisions/) records the decision. A roadmap item that
   replaces the ownership model — team membership — certainly needs one.
4. **Plan the work.** The item moves out of this document's `Proposed` column
   and into a phase, and the phase numbering is settled (`Q-11`).
5. **Implement it under the normal delivery workflow**, with the same quality
   gates, the same documentation protocol and the same review. A roadmap item
   is not a lower standard; it is a later one.

## 6. Related

- [`project-truth.md`](project-truth.md) — the accepted requirements, the
  exclusions and the open decisions.
- [`engineering/implementation-status.md`](engineering/implementation-status.md)
  — what exists today.
- [`engineering/open-questions.md`](engineering/open-questions.md) — the
  decisions that would change this document.
- [`engineering/known-issues.md`](engineering/known-issues.md) — the limits of
  what Phase 1 does.
- [`features/`](features/) — the feature documents of the accepted scope.
