# ADR-0001 — ATOM is a modular monolith

> **Status:** Accepted
> **Date:** 2026-10-21
> **Owner:** architect

## Context

ATOM is an API-first personal workspace platform. Its long-term shape has
three layers: a durable personal record (notes, projects, tasks and, later,
documents and activity), a workflow engine on top of that record (n8n
automations), and a personal intelligence layer (local machine learning and
retrieval over the user's own data).

Phase 1 delivers the backend foundation. The scope is defined in
[`../project-truth.md`](../project-truth.md) §3 and §4, and the exclusions in
§6 are explicit: no microservices, no message brokers, no event-streaming
infrastructure, no multi-tenant collaboration, and no "enterprise
abstractions that no current requirement needs".

The scale of the problem is small and known. Phase 1 has exactly one actor
(the registered owner), four resource families (`users`, `notes`, `projects`,
`tasks`) and one workflow: request in, validate, query, respond. There is a
single writer of the data — the API — and a single consumer — the client.

Splitting a system into services is not free. Every split introduces a
network call, a second failure mode, a second deployment, a schema or API
contract between the parts, an observability surface that crosses a process
boundary, and a distributed transaction wherever two services write the same
business fact. None of those costs is justified by a Phase 1 that has one
writer, one reader and no asynchronous work.

The requirement is also explicit that the build "must not require a hosted or
paid service" ([`../project-truth.md`](../project-truth.md) §8), and that the
documentation must be good enough for an open-source contributor to use the
project (§3). A design that requires a message broker, a service mesh or a
managed database to be understood and run fails both constraints for Phase 1.

## Decision

**ATOM is one deployable unit with strict internal module boundaries.**

- The Phase 1 backend is a **single process** built from the `atom-api`
  crate, with one binary and one PostgreSQL database.
- The code is organised as a **modular monolith**: `api`, `auth`, `users`,
  `notes`, `projects` and `tasks`, plus the cross-cutting modules
  `config`, `error`, `state`, `pagination`, `ratelimit`, `telemetry` and
  `health`.
- The layering inside every feature module is fixed: `handlers` → `service`
  → `repository` → PostgreSQL, with `models` for the persistence shapes and
  `dto` for the public contracts. A handler never talks to the database.
- The modules are separated by **types, not by processes**. A module
  communicates through its public interface — a service, a repository, a
  DTO — and never reaches into another module's internals.
- **No broker, no queue, no event bus, no background worker and no second
  service** exists in Phase 1. Every operation is a synchronous request and
  response.
- The boundaries are written down in
  [`../architecture/module-boundaries.md`](../architecture/module-boundaries.md)
  and the reasoning is recorded in
  [`../architecture/system-overview.md`](../architecture/system-overview.md).

## Alternatives considered

| Alternative | Why it was not chosen |
| ----------- | --------------------- |
| **Microservices** — one service per feature (`auth`, `notes`, `projects`, `tasks`), each with its own database and deployment. | It introduces a network hop, a partial failure mode and a data-consistency problem into every operation that is currently a single transaction, and it adds a second and third deployment to a project whose requirements explicitly exclude microservices. The boundaries are real, so the *option* to split later is preserved — but paying for it before it is needed is exactly the cost the requirements reject. |
| **Event-driven / asynchronous architecture** — every write publishes an event, and consumers project it into read models. | There is no second consumer in Phase 1. The only consumer is the client that made the request, and it is answered synchronously. An event bus would add a delivery guarantee, an ordering guarantee and an operational surface to a system that has no asynchronous work to do. |
| **Serverless functions** — one function per route, with the database accessed directly. | The functions would share one database and therefore share every consistency and ownership problem, while losing the in-process rate limiter, the shared configuration object and the single-process middleware stack. The cold-start and per-invocation costs are real for an API that a personal client polls. |
| **A modular monolith with a plugin or hook mechanism** — modules register hooks that others can call. | A hook registry is a hidden dependency graph. The type system already expresses "module A uses module B"; a runtime registry would make that graph invisible to the compiler and to the reviewer, for no Phase 1 requirement. |

## Consequences and trade-offs

### Positive

- **One transaction boundary.** A write that touches one table is one
  statement; a write that touches two is one process. There is no
  distributed transaction anywhere.
- **One deployment artifact.** One binary, one database, one configuration
  surface. A contributor can run the whole system with `docker compose up -d
  postgres` and `cargo run`.
- **The type system enforces the boundaries.** A module that reaches into
  another module's repository does not compile. This is the property that
  makes the modularity real rather than aspirational.
- **The cost of a later split is bounded.** The seams are already in the
  right place: `AuthService`, `NoteService`, `ProjectService` and
  `TaskService` are the natural extraction points, and the ownership model
  is already per user.
- **No broker to operate.** The operational surface of Phase 1 is a process
  and a database.

### Negative

- **A single process is a single failure domain.** If the process is down,
  the whole API is down. This is accepted: there is nothing else to serve.
- **Scaling is per process, not per feature.** A hot feature cannot be scaled
  independently. The Phase 1 workload is not hot in any part.
- **The module boundaries are enforced by convention and review, not by the
  compiler.** A feature module can call `sqlx` directly if a contributor
  writes it; the layering rule in [`../../AGENTS.md`](../../AGENTS.md) §2 and
  the review checklist are what prevent it.
- **The rate limiter is per process and therefore does not survive a
  horizontal scale.** This is the concrete cost of the single-unit decision
  and it is recorded as KI-001 in
  [`../engineering/known-issues.md`](../engineering/known-issues.md). It is
  the reason the production checklist requires edge rate limiting.
- **The future workflow layer will need a seam that does not exist yet.**
  The n8n automation planned for a later phase reacts to changes in the
  record. That will need either a change-data-capture mechanism, a polling
  API, or an outbox — a design that has not been decided and must not be
  pre-empted by this ADR.

## Security and operational implications

- **The trust boundary is between the client and one process.** There is no
  internal service-to-service traffic to authenticate, to encrypt or to
  authorise. The six boundaries in
  [`../architecture/security-architecture.md`](../architecture/security-architecture.md)
  are all external to the process or internal to it.
- **The whole trust surface of the data is in one place.** A defect in the
  authorization model is a defect in one process, and it is testable with
  one database. In a split system, the same defect would have to be
  reproduced and tested in every service that reads the data.
- **One audit trail.** The logs of every request land in one sink, with one
  correlation id, and there is no need to correlate across services.
- **One thing to secure.** There is no inter-service credential, no service
  account, no mesh policy and no second set of secrets.
- **Availability is the availability of one process.** The readiness probe
  and the pool are the only capacity controls.

## Related requirements and changes

| Reference | Relationship |
| --------- | ------------ |
| [`../project-truth.md`](../project-truth.md) §3 | The Phase 1 scope this decision serves. |
| [`../project-truth.md`](../project-truth.md) §8 | The technical constraints: Rust, Axum, PostgreSQL, fixed layering, no global mutable state. |
| [`../project-truth.md`](../project-truth.md) §6 | The exclusions: no microservices, no brokers, no event streaming. |
| [`../project-truth.md`](../project-truth.md) §15 | The dependency graph between the modules. |
| [`../architecture/module-boundaries.md`](../architecture/module-boundaries.md) | The boundaries this decision produces. |
| [`../architecture/system-overview.md`](../architecture/system-overview.md) | The runtime shape this decision produces. |
| [`../engineering/known-issues.md`](../engineering/known-issues.md) KI-001 | The per-process rate limiter, the cost of the single-unit decision. |

## Supersedes / Superseded by

Supersedes: none. This is the first architecture decision record.

Superseded by: none. A review of this decision is due when the roadmap
reaches the automation layer, because that layer will need a seam that this
decision deliberately does not create.
