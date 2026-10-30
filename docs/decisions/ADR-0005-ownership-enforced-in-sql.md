# ADR-0005 — Ownership is enforced in SQL

> **Status:** Accepted
> **Date:** 2026-10-21
> **Owner:** architect

## Context

Every private resource in ATOM belongs to exactly one user. Phase 1 is
single-owner by design: there is no sharing, no collaboration, no
multi-tenancy and no role model
([`../project-truth.md`](../project-truth.md) §7, §15 and §6).

The requirements are unambiguous about how that ownership must be enforced:

- "Every query that touches user data must be scoped by the caller's
  `user_id`. Ownership is enforced in SQL, not in the handler"
  ([`../../AGENTS.md`](../../AGENTS.md) §4.3);
- "A resource that the caller does not own must return `404`, never `403`"
  (the same rule, and the requirement in
  [`../project-truth.md`](../project-truth.md) §9.7);
- "Ownership checks on every private endpoint"
  ([`../project-truth.md`](../project-truth.md) §4.2);
- "Ownership is modelled as a non-null foreign key and enforced in every
  query" ([`../project-truth.md`](../project-truth.md) §12).

The shape of the data makes this a two-level problem. A note, a project and
a refresh session carry a `user_id` column directly. A task does not: a
task belongs to a project, and a project belongs to a user. The ownership of
a task is therefore *inherited*, and expressing it means joining through the
parent on every read, every write and every delete.

The alternative — checking ownership in the handler — is superficially
simpler, and it is the shape most frameworks encourage: fetch the row, look
at its owner field, compare, then reject. It has two well-known failure
modes. A handler that forgets the comparison leaks the resource. And a
handler that fetches the row *before* it checks ownership has already
executed a statement whose result it must not disclose, so the "not mine"
decision is made after the fact rather than by the database.

## Decision

**Every statement that reads or writes user data carries the caller's
`user_id` as a bound parameter. The ownership predicate is part of the
statement, and a resource the caller does not own is reported as `404`,
never `403`.**

### 1. The rules

1. **The predicate is in the SQL.** Every repository method that touches
   `notes`, `projects` or `tasks` takes the caller's `user_id` as a
   parameter and puts it in the `WHERE` clause.
2. **The handler never compares identifiers.** The handler passes
   `user.user_id` down and does nothing else. There is no ownership check
   outside the repository layer.
3. **A missing row is a `404`.** The repository returns no row; the service
   turns that into `AppError::NotFound`. A resource that does not exist and
   a resource that belongs to somebody else produce the **same** response.
4. **`403` is not used for an inaccessible resource.** It is used only for a
   state problem that the caller *is* entitled to know about: a disabled
   account, and a logout that targets another account's session.
5. **A task inherits its ownership through the project.** Every task
   statement joins to `projects` and filters on `projects.user_id`.
6. **The predicate is written as an explicit join or subquery**, not as a
   shared helper, so that a future team-membership model widens one
   predicate rather than every handler.

### 2. The shapes that follow

| Operation | The shape of the predicate |
| --------- | -------------------------- |
| Read | `WHERE id = $1 AND user_id = $2` |
| List | `WHERE user_id = $1` plus the filters |
| Update | `UPDATE … SET … WHERE id = $1 AND user_id = $2 RETURNING …` |
| Delete | `DELETE FROM … WHERE id = $1 AND user_id = $2`, and the result is the number of rows affected |
| Create a task | `INSERT … SELECT … FROM projects p WHERE p.id = $2 AND p.user_id = $3` — the check and the write are one statement |
| Update a task | `UPDATE tasks t SET … FROM projects p WHERE t.id = $1 AND t.project_id = p.id AND p.user_id = $2` |
| Delete a task | `DELETE FROM tasks t USING projects p WHERE t.id = $1 AND t.project_id = p.id AND p.user_id = $2` |

There are two deliberate exceptions, both of them lookups *by* an identity
rather than *of* a resource:

| Exception | Statement | Why |
| --------- | --------- | --- |
| `UserRepository::find_by_email` | `SELECT ... FROM users WHERE email = $1` | The account has to be looked up *by* its identity in order to authenticate it. The caller is not yet an authenticated user, so there is no owner to scope by. |
| `RefreshSessionRepository::find_by_token_hash` | `SELECT ... FROM refresh_sessions WHERE token_hash = $1` | The session has to be looked up *by* the presented token. The token is the credential; the owner is discovered from the row, and the owner is then verified against the caller before anything is mutated. |

Every mutation of a refresh session is scoped by the owner:
`RefreshSessionRepository::revoke` and `RefreshSessionRepository::claim` both
carry `AND user_id = $2` in their predicate, and `revoke_all_for_user` carries
the owner in its predicate. The lookup exception therefore cannot be used to
mutate another account's session.

**Amendment (2026-10-30).** The table above names the two exceptions the
decision originally reasoned about. Two further identity lookups exist today,
and they follow the same rule — the identity is not a client-supplied resource
id, and every mutation is owner-scoped — so they are recorded here rather than
left implicit:

| Exception | Statement | Why |
| --------- | --------- | --- |
| `UserRepository::find_by_id` | `SELECT ... FROM users WHERE id = $1` | The `id` is never a path parameter; it is the subject of the presented credential (the token's `sub`, or the session's owner during a refresh). It is the same "the caller must be resolved before anything can be scoped" situation as `find_by_email`, with the identity arriving from the token rather than the body. |
| `ApiKeyRepository::find_identity_by_hash` | `SELECT ... FROM api_keys k JOIN users u ... WHERE k.key_hash = $1` | The presented key material is the credential; the owner is discovered from the row and then verified (revoked, account active) before anything acts. Recorded in [`ADR-0011`](ADR-0011-api-keys.md). |

### 3. The `404` over `403` rule

Returning `403` for a resource that belongs to somebody else tells the
caller that the resource exists. That is a disclosure of another account's
data — the existence of a note, a project or a task — and it turns the
identifier space into an oracle. `404` makes the two cases
indistinguishable.

The rule has a practical consequence that is easy to get wrong: the service
must not distinguish "not found" from "not yours". Both are
`AppError::NotFound` with the same message.

### 4. The task list and the empty result

`TaskRepository::list_for_project` joins to the project, so an
unauthorized project produces no rows — the same as a project with no
tasks. The repository therefore performs a follow-up ownership probe when the
result is empty, so that the service can answer "no such project" with a
`404` and "a project with no tasks" with an empty list, without ever
disclosing a project the caller does not own.

## Alternatives considered

| Alternative | Why it was not chosen |
| ----------- | --------------------- |
| **Checking ownership in the handler** — fetch the row, compare `user_id` with the caller's, return `403` otherwise. | It puts a security decision in a layer that has no compiler support for it, so every new handler is a chance to forget the comparison. It also executes the read *before* the decision, so the row has already been loaded into the process. It is exactly the shape that produces broken object-level authorization, which is threat T-05 in [`../security/threat-model.md`](../security/threat-model.md). |
| **Returning `403` for an inaccessible resource** | It discloses that the resource exists. The requirement in [`../project-truth.md`](../project-truth.md) §9.7 is `404`, and the reason is the disclosure. |
| **PostgreSQL row-level security** — a policy per table, with the session user set to the caller's id. | It would make the database itself the enforcement point, which is stronger than any application convention. It was not chosen for Phase 1 because it requires a dedicated database role per caller (or a `SET LOCAL` on every transaction), it interacts badly with a connection pool that reuses sessions, and it would put the identity of the caller into the session state — a much larger change than the requirement asks for. It remains the natural answer if ATOM ever grows a multi-tenant model, and the decision would be taken then, with the operational cost priced in. |
| **A central authorization service** — every query goes through a component that decides what the caller may see. | It adds a network hop or a shared cache to the hot path, it re-introduces the "the check is a separate step" problem, and it becomes a single point of failure for every read in the system. |
| **An ORM with a per-request scope** — the ORM injects the owner filter automatically. | It hides the predicate from the reviewer. The security property this project depends on is precisely that the predicate is *visible in the statement* and checkable at a glance. An invisible filter is a filter nobody reviews. |
| **Carrying the owner id in the URL** — `/api/v1/users/{user_id}/notes` | It makes the ownership explicit in the contract and invites the client to name another user. The caller does not get to say who they are. |

## Consequences and trade-offs

### Positive

- **One place to get right.** The predicate is in the repository, and the
  repository is a small, reviewable file per feature.
- **The decision is made by the database.** A statement that does not match
  returns no row; there is no in-process row that has to be filtered later.
- **No existence oracle.** `404` for both cases, with the same message.
- **The invariant is testable.** The integration suite contains a
  cross-account isolation test for every private endpoint, and each test
  would fail if the predicate were removed from the statement.
- **A task cannot be attached to somebody else's project.** The insert is a
  single statement, so the check and the write cannot be separated.
- **Widening the model later touches one place.** A team-membership model
  replaces `p.user_id = $n` with an `EXISTS` over the membership table. The
  handler, the service and the DTOs do not change.

### Negative

- **The statements are more complex.** A task read is a join, a task update
  is an `UPDATE … FROM`, and a task delete is a `DELETE … USING`. A
  reviewer has to read more SQL than the naive version.
- **The task repository needs the join in every method.** Five methods, five
  joins. There is no way to avoid that without an ORM that would hide the
  predicate, and the duplication is deliberate.
- **The predicate is a convention, not a compiler guarantee.** Nothing stops
  a future contributor from writing a statement without it. The mitigation
  is the secure-development rule SD-2, the review checklist and the
  isolation tests — none of which is as strong as a type error.
- **A missing predicate is silent.** It does not fail to compile and it does
  not fail loudly; it returns rows. This is the residual risk that T-05 in
  [`../security/threat-model.md`](../security/threat-model.md) names
  explicitly.
- **`403` is almost unused.** The status is available for a state problem
  the caller is entitled to know about, which is a narrow category, and a
  reviewer has to remember that it is not the "not mine" answer.
- **The empty-list case needs an extra probe.** Distinguishing "no such
  project" from "no tasks" costs a second statement in the empty case.

## Security and operational implications

- **Broken object-level authorization becomes a query-review problem.** The
  thing that must never happen — reading another account's data — is
  prevented by a predicate that appears in the statement, next to the data
  it selects.
- **The `404` rule closes an enumeration channel.** Even with a valid
  token, a caller cannot learn whether a given identifier exists.
- **The database role does not have to be clever.** The role needs the
  ordinary table privileges; the authorization is in the statements, not in
  a row-level policy.
- **Pooled connections stay simple.** There is no per-caller session state
  to leak between pool users, which is the operational advantage of this
  decision over row-level security.
- **The isolation tests are a security control.** The cross-account tests
  are not just coverage; they are the regression guard for the property.
- **The property survives a multi-instance deployment.** The enforcement is
  in the statement, so it does not depend on shared state.

## Related requirements and changes

| Reference | Relationship |
| --------- | ------------ |
| [`../project-truth.md`](../project-truth.md) §9.6, §9.7 | Ownership in SQL, and `404` instead of `403`. |
| [`../project-truth.md`](../project-truth.md) §12 | Ownership as a non-null foreign key, enforced in every query. |
| [`../project-truth.md`](../project-truth.md) §15 | The dependency graph: `tasks` inherits its authorization from `projects`. |
| [`../../AGENTS.md`](../../AGENTS.md) §4.3 | The engineering rule this decision implements. |
| [`../security/secure-development.md`](../security/secure-development.md) SD-2, SD-3 | The rules that make the decision checkable. |
| [`../security/threat-model.md`](../security/threat-model.md) T-05 | The threat the decision mitigates, and its residual risk. |
| [`../architecture/data-architecture.md`](../architecture/data-architecture.md) §2 | The ownership model this decision produces. |
| [`../architecture/request-lifecycle.md`](../architecture/request-lifecycle.md) §5.1 | Where the authorization happens in the request path. |
| [`../decisions/ADR-0003-auth-token-design.md`](ADR-0003-auth-token-design.md) | The decision that supplies the caller's identity. |

## Supersedes / Superseded by

Supersedes: none. This is the first decision about authorization.

Superseded by: none. A review of this decision is due when the team-membership
model is accepted, because that model widens the predicate and needs its own
ADR.
