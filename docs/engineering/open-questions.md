# ATOM open questions

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-24
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

**The implementation now matches the absolute-lifetime answer that
[`../decisions/ADR-0003-auth-token-design.md`](../decisions/ADR-0003-auth-token-design.md)
already records.** ADR-0003 decided that there is one absolute lifetime and no
idle timeout, and the rotation preserves it: the replacement session inherits
the original `expires_at`, so a rotation never extends the absolute lifetime
of a session. The part of the question that is still open is the **idle
timeout**: whether a session that is not used for a while should expire
earlier than its absolute deadline.

**Options.**

| Option | Impact |
| ------ | ------ |
| Keep one absolute lifetime and no idle timeout. | The current behaviour, and the decision ADR-0003 already records. A stolen token is worthless after the lifetime, and the owner never has to think about it. The cost is that a long-lived session on a device that is no longer used stays valid until the deadline. |
| Add an idle timeout on top of the absolute lifetime. | A session that is unused for, say, 30 days expires. The cost is an extra column, an extra predicate in the session query, and a rule for what the idle timeout is. |
| Add a sliding absolute lifetime: each rotation extends the deadline. | The owner is never logged out. The cost is that a stolen token can be kept alive indefinitely by the thief, which removes the upper bound that makes the absolute lifetime valuable. **This option contradicts ADR-0003** and would need a new decision that supersedes it. |

**Who decides.** The product owner, with the security specialist.

**Related.** [`../project-truth.md`](../project-truth.md) §16 Q3;
[`../decisions/ADR-0003-auth-token-design.md`](../decisions/ADR-0003-auth-token-design.md);
`KI-015` in [`known-issues.md`](known-issues.md), the defect that let a
rotation extend the absolute lifetime.

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
[`../../src/notes/`](../../src/notes/).

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

**Context.** The API never reads `X-Forwarded-For`, which is correct: the
header is client-controlled. The server installs the connection information,
so the rate limiter is keyed by the **peer address** of the connection
(`KI-010` is Closed). What the API cannot do is tell a proxy from a client:
there is no trusted-proxy declaration anywhere, so behind a reverse proxy
that does not terminate the connection every request carries the proxy's
address and the budget is shared again (`KI-003`). The requirement
[`../project-truth.md`](../project-truth.md) §9.11 says the API "never reads
the `X-Forwarded-For` header, and there is no trusted-proxy declaration" —
the first half is implemented, and the second half is the open question.

**Options.**

| Option | Impact |
| ------ | ------ |
| The proxy terminates the connection, and the API reads the peer address from the socket. | The simplest correct answer, and the one the current design already implements. The cost is that the topology must not have the proxy in front of a proxy without the last hop terminating, so it constrains the deployment rather than the code. |
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

**Context.** [`../../Cargo.toml`](../../Cargo.toml) declares **no** licence at
all. The manifest carries the comment "The licence has not been chosen yet."
and has no `license` key; the only licensing statement in it is
`publish = false`, which keeps the crate from being published to a registry by
accident. No `LICENSE` file is committed either.
[`../../README.md`](../../README.md) and
[`../../CONTRIBUTING.md`](../../CONTRIBUTING.md) §9 agree with the manifest:
both record that the licence has not been chosen yet, and both list "choose a
licence and add the corresponding `LICENSE` file" as a step that is still to
be taken.

The decision the owner has to make is therefore **not** "add the missing
`LICENSE` file". Neither the file nor the manifest key is present, and no
part of the repository depends on the licence being named. What is missing is
the **choice itself**: until it is made, the manifest cannot carry a
`license` key, no `LICENSE` file can be written, and the two cannot be made
to agree.

**Options.**

| Option | Impact |
| ------ | ------ |
| MIT. | Permissive, well understood, and the most common choice for a small Rust project. The cost is that the choice has to be made deliberately, that a `LICENSE` file must be committed, and that the manifest and the file must then agree. |
| Apache-2.0. | Permissive, with an explicit patent grant. The cost is a longer file, and a `NOTICE` convention to follow. |
| AGPL. | Requires that a modified deployment is offered to its users. The cost is a much stronger obligation on anyone who forks the project, and it is a decision the maintainer has to make deliberately. |
| Keep it unlicensed. | The current de facto state. The cost is that nobody may legally use, fork or contribute to the project, which contradicts the open-source goal. |

**Who decides.** The project owner.

**Related.** [`../../CONTRIBUTING.md`](../../CONTRIBUTING.md) §9;
[`../../README.md`](../../README.md);
[`../../Cargo.toml`](../../Cargo.toml).

### Q-11 — What are the phases after Phase 1?

**State:** Answered · **Blocks:** nothing any more

**Answer (2026-10-24, intake `D1`).** [`../roadmap.md`](../roadmap.md) §2 now
defines the phases: Phase 1 is the backend foundation; **Phase 2 is the personal
workspace** — reimbursements, job-application tracking, local machine learning
and the database seeder — with its requirements recorded in
[`../project-truth.md`](../project-truth.md) §18 and its delivery waves in
[`runs/chain/plan.md`](runs/chain/plan.md) §7. Phase 3 and later stay
unassigned text in the roadmap, so this answer commits to one phase, not to a
numbered plan for everything.

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

### Q-12 — Should a deployment be able to reload a configuration change without a restart, and which settings may change at run time?

**State:** Open · **Blocks:** nothing today

**Context.** The configuration was centralised in the `config/` directory and
resolved once at start-up by
[`../../src/config/loader.rs`](../../src/config/loader.rs). A deployment
therefore changes a setting by writing a file or by setting an environment
variable, and the change takes effect only after a restart. The settings that
are expensive to change at run time are not the same as the ones that are
dangerous to change at run time: the request timeout and the rate limit could
plausibly be adjusted without a restart, while the signing secret, the
database URL and the pagination limits cannot be changed under a running
process without changing what the running process means. The related question
is whether the trusted-proxy list — the design of which is `Q-07` — becomes a
configuration setting; it is currently not a setting at all.

**Options.**

| Option | Impact |
| ------ | ------ |
| No reload at all. The configuration is resolved once, at start-up, and a change needs a restart. | The current decision. The cost is that a deployment cannot react to a change without a restart, and a supervised process must be restarted deliberately. The cost of a reload — a partially applied configuration, a secret that changes under a running process — never arises. |
| A reload of the non-secret settings only. The secrets, the pagination limits and the bind address stay fixed for the life of the process; the rest can be re-read. | A deployment can raise the rate limit or shorten the request timeout without a restart. The cost is a defined subset of reloadable settings, a mechanism to trigger the reload, and the question of what happens to in-flight requests when a limit changes. |
| A full reload, including the secrets. | Everything can change without a restart. The cost is the largest: a secret that changes under a running process invalidates the tokens that were signed with the old one, and a reload that fails halfway leaves the process in a state that is neither the old configuration nor the new one. |

**Who decides.** The product owner, with the architect and the `devops`
specialist.

**Related.** `KI-014` in [`known-issues.md`](known-issues.md); `Q-07`;
[`../../config/README.md`](../../config/README.md);
[`../architecture/deployment.md`](../architecture/deployment.md).

### Q-13 — Are the Phase 2 working assumptions correct?

**State:** Open · **Blocks:** nothing in Phase 2's engineering; it blocks the product behaviour each assumption describes

**Context.** The Phase 2 intake rests on six working assumptions — `A1`–`A6` in
[`runs/chain/plan.md`](runs/chain/plan.md) §3 — that were **not explicitly
confirmed** by the owner (owner decision `O1`, recorded in
[`runs/chain/ledger.md`](runs/chain/ledger.md)). The intake therefore recorded
them as **provisional defaults**: a claim with line items and one currency per
claim (`A1`), receipts as references rather than uploaded files (`A2`),
applications tracked but never generated (`A3`), machine learning local only
(`A4`), a single owner with no teams (`A5`), and money as `NUMERIC(19,4)` and a
JSON string (`A6`). Each is stated as provisional where it is used; the origin
note is [`../project-truth.md`](../project-truth.md) §18.4.

**Options.**

| Option | Impact |
| ------ | ------ |
| Confirm all six. | The provisional marker is removed from `project-truth.md` §18, and the specifications are promoted from `Accepted` as written. Cheapest for the build; it makes the build's shape the product's shape. |
| Confirm some, revise others. | Each revision is a new change against the affected specification and, where it changes money or storage, an amendment to `ADR-0007` or `ADR-0009`. Nothing is rewritten silently. |
| Leave them provisional. | The features stay `Accepted` and their behaviour is built to the provisional reading; a later revision is a normal change. This is the state today. |

**Who decides.** The product owner.

**Related.** [`../project-truth.md`](../project-truth.md) §18.4;
[`runs/chain/plan.md`](runs/chain/plan.md) §3;
[`../decisions/ADR-0007-money-representation.md`](../decisions/ADR-0007-money-representation.md);
[`../decisions/ADR-0009-receipt-references.md`](../decisions/ADR-0009-receipt-references.md).

### Q-14 — Is `ML-R03`'s "every automated test uses `Deterministic`" meant literally?

**State:** Open · **Blocks:** nothing; the behaviour is implemented and tested either way

**Context.** `ML-R03` in [`../project-truth.md`](../project-truth.md) §18.3 says
the embedder is a trait with three implementations and that "every automated test
uses `Deterministic`". Read literally that is false, and the tree is not going to
make it true without weakening a test: six cases in `src/ml/burn_embedder.rs`
exercise the encoder's arithmetic with synthetic tensors, and fifteen in
`src/ml/tokenizer.rs` exercise the tokenizer with a fixture vocabulary. Neither
loads a model file or performs I/O, which is the property the requirement exists
to protect - "no test downloads a model, no test calls out". The forty-one
`tests/ml_api.rs` cases, which do exercise search and matching end to end, all use
`Deterministic`.

**The question is whether the requirement means** (a) *no automated test may
depend on the real model or a network*, which is what the tree satisfies, or
(b) *no automated test may instantiate any embedder other than `Deterministic`*,
which would forbid the encoder's own unit tests and leave its arithmetic untested.

**Options.**

| Option | Impact |
| ------ | ------ |
| Read it as (a) and amend the wording. | One sentence in §18.3 plus the feature specification; no code change, and the encoder keeps its tests. Requires the owner, because it edits an accepted requirement. |
| Read it as (b) and delete the encoder/tokenizer unit tests. | The encoder's arithmetic loses all coverage; the suite would still pass. Not recommended, and not done unilaterally. |
| Leave the text and the finding as they are. | This is the state today: the discrepancy is recorded in [`../features/ml/status.md`](../features/ml/status.md) §2 and nothing claims otherwise. |

**Who decides.** The product owner.

**Related.** [`../project-truth.md`](../project-truth.md) §18.3 `ML-R03`;
[`../features/ml/status.md`](../features/ml/status.md) §2;
[`../features/ml/testing.md`](../features/ml/testing.md).

---

## Summary

| ID | Question | State | Blocks |
| -- | -------- | ----- | ------ |
| `Q-01` | Deleted project: delete the tasks or archive them? | Open | A change to the delete behaviour |
| `Q-02` | Refresh token: response body or HTTP-only cookie? | Open | The browser client design |
| `Q-03` | Refresh-token absolute lifetime and idle timeout | Open | The session policy — the absolute lifetime is settled by ADR-0003, the idle timeout is not |
| `Q-04` | Note search: ranking and highlighting? | Open | The search experience |
| `Q-05` | Web client cookies and the CSRF strategy | Open | The web client, not the API |
| `Q-06` | Task ordering semantics and a reordering endpoint | Open | A change to the task ordering |
| `Q-07` | How a trusted proxy is declared | Open | The deployment topology |
| `Q-08` | A password-change and session-management surface | Open | Nothing today |
| `Q-09` | The rate limiter: fail open or closed, and which algorithm | Open | Nothing today |
| `Q-10` | Which license does ATOM use? | Open | The publication of the repository |
| `Q-11` | What are the phases after Phase 1? | **Answered** — Phase 2 defined in `roadmap.md` §2 | Nothing any more |
| `Q-12` | Reload without a restart, and which settings may change at run time | Open | Nothing today |
| `Q-13` | Are the Phase 2 working assumptions (`A1`-`A6`) correct? | Open | The product behaviour each assumption describes |
| `Q-14` | Is `ML-R03`'s "every automated test uses `Deterministic`" literal? | Open | Nothing; the encoder's unit tests depend on the answer |
