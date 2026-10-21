# ATOM open questions

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-21
> **Owner:** maintained by the `docs` agent

Product and architecture decisions that have not been taken yet. Each question
states the context, the options, what each option costs, and who should decide.

**None of the questions below blocks Phase 1 work that does not depend on the
answer.** The work that does depend on an answer waits for it; everything else
proceeds. That is the rule in [`../project-truth.md`](../project-truth.md) §16
and in [`../../AGENTS.md`](../../AGENTS.md) §5: record the conflict, continue
with the work that does not depend on it, and ask a human only when a material
decision genuinely cannot be made safely.

A question that turns out to be a defect rather than a decision is moved to
[`known-issues.md`](known-issues.md). A question that is answered is answered
in [`../project-truth.md`](../project-truth.md), and the answer is recorded
there with the requirement it settles.

---

## The format

| Field | Meaning |
| ----- | ------- |
| **ID** | `Q-NN`, never reused. |
| **State** | `Open` for every question below. A question that is answered becomes `Answered`, with the date and the place the answer was recorded. |
| **Context** | What is known today, and what forces the question. |
| **Options** | The real options, each with what it costs. |
| **Who decides** | The role that owns the decision. |
| **Blocks** | The work that waits. Usually nothing. |

---

## The questions

### Q-01 — Should a deleted project delete its tasks, or archive them?

**State:** Open · **Blocks:** a change to the delete behaviour

**Context.** The schema cascades: `tasks.project_id` is
`REFERENCES projects (id) ON DELETE CASCADE`, so deleting a project deletes its
tasks. The current behaviour is verified by
`deleting_a_project_removes_its_tasks` in
[`../../tests/projects_tasks_api.rs`](../../tests/projects_tasks_api.rs).
The requirement
[`../project-truth.md`](../project-truth.md) §16 Q1 asks whether that is the
right behaviour.

**Options.**

| Option | Impact |
| ------ | ------ |
| Keep the cascade. A deleted project takes its tasks with it. | No change. A task has no home without its project, and the schema already says so. The cost is that a deleted task cannot be recovered. |
| Archive the tasks instead of deleting them. | The task table needs a soft-delete column, and every task query needs the extra predicate. The owner keeps a recoverable history, at the cost of a migration, a wider query surface and a second concept of "exists". |
| Delete the project and let the client decide what to do with the tasks first. | The API exposes the tasks before the delete so the client can move them. The cost is a client-side dance for something the server can do atomically. |

**Who decides.** The product owner.

**Related.** [`../project-truth.md`](../project-truth.md) §16 Q1 and §4.4.

### Q-02 — Should the refresh token live in an HTTP-only cookie instead of the response body?

**State:** Open · **Blocks:** the browser client design

**Context.** Today the refresh token is returned in the response body together
with the access token, and only its hash is stored server-side. The
requirement [`../project-truth.md`](../project-truth.md) §16 Q2 asks whether
that should change.

**Options.**

| Option | Impact |
| ------ | ------ |
| Keep the response body. | The current behaviour. It works for a non-browser client and for a mobile client, and it introduces no CSRF surface. The cost is that a browser client must keep the token in scriptable storage, where any script on the page can read it. |
| Move the refresh token to an HTTP-only, `Secure`, `SameSite` cookie. | The token is no longer reachable from script. The cost is that every refresh request becomes a cross-site request forgery candidate, so a CSRF defence — a double-submit token, a strict `SameSite`, or a custom header check — becomes mandatory, and the API's endpoints change shape. |
| Issue the refresh token in the body but require the client to send it in a custom header. | A partial mitigation: a custom header forces a preflight, which a cross-origin form cannot produce. The cost is that the token is still in scriptable storage, and non-browser clients must learn the custom header. |

**Who decides.** The product owner, together with the design of the web
client.

**Related.** `KI-007` in [`known-issues.md`](known-issues.md); accepted risk
`AR-2` in [`../security/threat-model.md`](../security/threat-model.md);
[`../project-truth.md`](../project-truth.md) §16 Q2.

### Q-03 — What is the refresh-token absolute lifetime and idle timeout?

**State:** Open · **Blocks:** the session policy

**Context.** A refresh session expires at `expires_at` and is never extended.
The default lifetime is 14 days, configured by
`APP_REFRESH_TOKEN_TTL_SECONDS`, and the configuration requires it to be longer
than the access-token lifetime. There is no idle timeout: a session that is
used every day still dies on its absolute expiry.

**Options.**

| Option | Impact |
| ------ | ------ |
| Keep one absolute lifetime and no idle timeout. | The current behaviour. A stolen token is worthless after the lifetime, and the owner never has to think about it. The cost is that a long-lived session on a device that is no longer used stays valid until the deadline. |
| Add an idle timeout on top of the absolute lifetime. | A session that is unused for, say, 30 days expires. The cost is an extra column, an extra predicate in the session query, and a rule for what the idle timeout is. |
| Add a sliding absolute lifetime: each rotation extends the deadline. | The owner is never logged out. The cost is that a stolen token can be kept alive indefinitely by the thief, which removes the upper bound that makes the absolute lifetime valuable. |

**Who decides.** The product owner, with the security specialist.

**Related.** [`../project-truth.md`](../project-truth.md) §16 Q3.

### Q-04 — Should note search support ranking and highlighting?

**State:** Open · **Blocks:** the search experience

**Context.** Search is a plain full-text match: `search_vector @@
plainto_tsquery('simple', $3)` over the title and the content, with the
`simple` configuration. There is no ranking and no highlighting, and the
listing is ordered by `created_at DESC, id DESC` regardless of the search.

**Options.**

| Option | Impact |
| ------ | ------ |
| Keep the plain match and the creation-time order. | The current behaviour. The cost is that a search for a common word returns a long, unranked list, and the caller cannot tell why one result came first. |
| Rank with `ts_rank` and order the results by relevance. | The list becomes useful for a broad query. The cost is a change to the ordering, which affects pagination, and a decision about whether the ranking is stable across pages. |
| Rank and highlight the matching terms. | The best experience. The cost is that the response shape grows a highlight structure, which is a public contract change, and a rendering decision that the API should not be making. |

**Who decides.** The product owner.

**Related.** [`../project-truth.md`](../project-truth.md) §16 Q4;
[`../features/notes/`](../../src/notes/).

### Q-05 — Does the future web client need refresh tokens in cookies, and does ATOM need a CSRF strategy?

**State:** Open · **Blocks:** the web client, not the API

**Context.** This is the client-side half of `Q-02`. It asks what the browser
client needs, and therefore what the API must offer. The API is already
CORS-enabled against an explicit origin allowlist for a future web client, and
credentials are not enabled.

**Options.**

| Option | Impact |
| ------ | ------ |
| The web client keeps the refresh token in scriptable storage. | No API change, no CSRF surface. The cost is that the token is reachable by any script on the page, so the web client must be strict about what it loads. |
| The web client uses an HTTP-only cookie and the API adds a CSRF defence. | The strongest browser posture. The cost is an API change, a cookie contract, and a CSRF token or an equivalent mechanism that must be designed and tested. |
| The web client does not use the refresh endpoint at all and drives a longer access-token lifetime instead. | No CSRF surface and no long-lived token in the browser. The cost is a weaker access-token model, which changes the security properties of the whole API. |

**Who decides.** The product owner, with the design of the web client.

**Related.** `Q-02`; [`../project-truth.md`](../project-truth.md) §16 Q5 and
§10.

### Q-06 — What are the task ordering semantics, and is there a reordering endpoint?

**State:** Open · **Blocks:** a change to the task ordering

**Context.** The `position` column is a `DOUBLE PRECISION` fraction, settable
by the client, with no uniqueness, no monotonicity rule and no endpoint that
reorders a project. See `KI-009` in
[`known-issues.md`](known-issues.md) for the concrete consequences. The
requirement [`../project-truth.md`](../project-truth.md) §4.4 asks for "manual
ordering inside a project".

**Options.**

| Option | Impact |
| ------ | ------ |
| Keep the fractional key, and add a dedicated reordering endpoint that computes the new positions. | The server owns the ordering, and the client sends an order rather than a number. The cost is a new endpoint, a transaction, and a rule for how the fractions are recomputed. |
| Keep the fractional key and let the client set `position` itself, but document the constraint and make the server reject a duplicate position. | No new endpoint. The cost is a write-check on every update, and the client still has to compute the fractions. |
| Replace the fractional key with an integer sequence, and reorder by rewriting the whole list. | Simple, exact and easy to reason about. The cost is a migration of the column, a write amplification on every reorder, and a conflict rule for two clients reordering at once. |
| Replace the fractional key with a lexicographic key. | Insertion in the middle without rewriting the neighbours. The cost is a migration, a less obvious column, and a comparison rule the client must not get wrong. |

**Who decides.** The product owner, with the architect.

**Related.** `KI-009` in [`known-issues.md`](known-issues.md);
[`../project-truth.md`](../project-truth.md) §4.4.

### Q-07 — How is a trusted proxy declared, so that the client address is real?

**State:** Open · **Blocks:** the deployment topology

**Context.** The API refuses to trust `X-Forwarded-For`, which is correct: the
header is client-controlled. But the server also does not install the
connection information, so every request currently shares one rate-limit
bucket (`KI-010`). The requirement
[`../project-truth.md`](../project-truth.md) §9.11 says the API "refuses to
trust `X-Forwarded-For` unless a trusted proxy sets it" — the second half of
that sentence has no implementation.

**Options.**

| Option | Impact |
| ------ | ------ |
| The proxy terminates the connection, and the API reads the peer address from the socket. | The simplest correct answer, and the one the current design is closest to. The cost is that the API must be given the connection information, and the topology must not have the proxy in front of a proxy without the last hop terminating. |
| The API is given a list of trusted proxy addresses, and it walks the `X-Forwarded-For` chain from the right, stopping at the first address it does not trust. | The standard approach, and it supports a proxy chain. The cost is a configuration surface, a parsing rule, and the risk of a parsing mistake that re-opens the spoofing hole. |
| The edge limiter is the only rate limit, and the API's limiter stays a backstop with a shared key. | The simplest to operate. The cost is that the in-process limiter is close to useless, and the API is weaker if the edge is missing. |

**Who decides.** The product owner, with the architect and the `devops`
specialist.

**Related.** `KI-001`, `KI-003`, `KI-010` in
[`known-issues.md`](known-issues.md);
[`../project-truth.md`](../project-truth.md) §9.10 and §9.11.

### Q-08 — Is there a password-change and session-management surface?

**State:** Open · **Blocks:** nothing today

**Context.** The Phase 1 scope has registration, login, refresh and logout.
There is no way to change a password, no way to list the active sessions, no
way to revoke a single session other than the one being logged out, and no
password-reset flow. A user who suspects a compromise can only log out the
session they happen to be using, or wait for the access token to expire.

**Options.**

| Option | Impact |
| ------ | ------ |
| Add the surface in a later phase. | The Phase 1 scope stays as accepted. The cost is that the owner has no in-product way to respond to a suspected compromise. |
| Add password change and session listing to Phase 1. | The owner can respond to a compromise. The cost is a new endpoint set, a policy for what happens to the other sessions when the password changes, a re-authentication requirement, and a new set of tests including the cross-account cases. |
| Add only a "revoke every session" endpoint, and leave the password alone. | The smallest useful addition. The cost is that a compromised password still has to be changed, and there is no way to do it. |

**Who decides.** The product owner.

**Related.** [`../project-truth.md`](../project-truth.md) §4.2;
`KI-012` in [`known-issues.md`](known-issues.md).

### Q-09 — Should the rate limiter fail closed when its lock is poisoned, and which algorithm should it use?

**State:** Open · **Blocks:** nothing today

**Context.** Two properties of the limiter are currently decisions rather than
accidents: it fails **open** on a poisoned lock (`KI-008`), and it uses a
**fixed window** rather than a sliding window or a token bucket. A fixed
window allows a burst of twice the limit across a window boundary. The
algorithm is a deliberate choice, documented in
[`../../src/ratelimit.rs`](../../src/ratelimit.rs) as intentionally simple so
that the behaviour is predictable and testable.

**Options.**

| Option | Impact |
| ------ | ------ |
| Keep both as they are. | The current behaviour. The cost is the window-boundary burst and a rate limit that stops applying after a panic. |
| Fail closed on a poisoned lock. | The limit always applies. The cost is that a single panic can deny authentication until the process is restarted, which for a single-owner deployment is a denial of service against the owner. |
| Move to a sliding window or a token bucket. | A smoother limit without the boundary burst. The cost is a more complex algorithm, more state, and more to test. |
| Replace the in-process limiter with a shared store. | The limit is correct across instances. The cost is a new dependency, a new failure mode, and a dependency the deployment must operate. |

**Who decides.** The product owner, with the `auth-security` specialist.

**Related.** `KI-001`, `KI-008` in [`known-issues.md`](known-issues.md);
accepted risks `AR-1` and `AR-7` in
[`../security/threat-model.md`](../security/threat-model.md).

### Q-10 — Which license does ATOM use?

**State:** Open · **Blocks:** the publication of the repository

**Context.** [`../../Cargo.toml`](../../Cargo.toml) declares
`license = "MIT"`, `publish = false` and no `LICENSE` file is committed.
[`../../CONTRIBUTING.md`](../../CONTRIBUTING.md) §9 records that the license
has not been chosen yet, and that the crate is marked `publish = false` so it
cannot be published by accident.

**Options.**

| Option | Impact |
| ------ | ------ |
| MIT, as the manifest already declares. | Permissive, well understood, and consistent with the manifest. The cost is that it must actually be chosen, a `LICENSE` file must be committed, and the manifest and the file must agree. |
| Apache-2.0. | Permissive, with an explicit patent grant. The cost is a longer file, and a `NOTICE` convention to follow. |
| AGPL. | Requires that a modified deployment is offered to its users. The cost is a much stronger obligation on anyone who forks the project, and it is a decision the maintainer has to make deliberately. |
| Keep it unlicensed. | The current de facto state. The cost is that nobody may legally use, fork or contribute to the project, which contradicts the open-source goal. |

**Who decides.** The project owner.

**Related.** [`../../CONTRIBUTING.md`](../../CONTRIBUTING.md) §9;
[`../../Cargo.toml`](../../Cargo.toml).

### Q-11 — What are the phases after Phase 1?

**State:** Open · **Blocks:** the roadmap's phase column

**Context.** ATOM defines one delivery phase today: Phase 1, the backend
foundation, described in [`../project-truth.md`](../project-truth.md) §3. The
roadmap in [`../roadmap.md`](../roadmap.md) lists the future features and
their dependencies, but no phase numbering beyond Phase 1, so the phase
column in that document is marked as unassigned.

**Options.**

| Option | Impact |
| ------ | ------ |
| Keep one phase. The roadmap is ordered by dependency only. | The simplest, and it keeps the phase column out of the requirements. The cost is that a large roadmap has no grouping, and a reader cannot tell what is "next". |
| Define Phase 2 as "the first client plus the first integration". | A useful grouping. The cost is a decision about which items belong to it, and a promise about ordering. |
| Define a phase per roadmap wave. | The roadmap becomes a plan. The cost is the most commitment, and the most to revise when a dependency changes. |

**Who decides.** The product owner.

**Related.** [`../roadmap.md`](../roadmap.md);
[`../project-truth.md`](../project-truth.md) §3 and §5.

---

## Summary

| ID | Question | State | Blocks |
| -- | -------- | ----- | ------ |
| `Q-01` | Deleted project: delete the tasks or archive them? | Open | A change to the delete behaviour |
| `Q-02` | Refresh token: response body or HTTP-only cookie? | Open | The browser client design |
| `Q-03` | Refresh-token absolute lifetime and idle timeout | Open | The session policy |
| `Q-04` | Note search: ranking and highlighting? | Open | The search experience |
| `Q-05` | Web client cookies and the CSRF strategy | Open | The web client, not the API |
| `Q-06` | Task ordering semantics and a reordering endpoint | Open | A change to the task ordering |
| `Q-07` | How a trusted proxy is declared | Open | The deployment topology |
| `Q-08` | A password-change and session-management surface | Open | Nothing today |
| `Q-09` | The rate limiter: fail open or closed, and which algorithm | Open | Nothing today |
| `Q-10` | Which license does ATOM use? | Open | The publication of the repository |
| `Q-11` | What are the phases after Phase 1? | Open | The roadmap's phase column |
