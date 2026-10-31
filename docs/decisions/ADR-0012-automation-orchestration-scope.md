# ADR-0012: Automation is orchestrated by an external n8n instance that reaches ATOM only through authenticated endpoints

> **Status:** `Accepted` — 2026-10-31, owner decision `O7` recorded (proposed
> 2026-10-30 and accepted as written; this line is the acceptance record — the
> body below, including its proposal wording, is kept unrewritten as the
> history of what was agreed)
> **Date:** 2026-10-30
> **Owner:** product owner

An architecture decision record. It records **one** decision, the options that
were considered, and the consequences that follow. It is never rewritten: a
decision that is replaced gets a new ADR, and this one is marked `Superseded`.

**This ADR is a proposal, not an agreement.** *[Reader's note, added
2026-10-31: this sentence and the proposal voice below are the record of
what was proposed — they are superseded by the `Accepted` status header
above; the wording is preserved unrewritten as the history of what was
agreed.]* [`../project-truth.md`](../project-truth.md)
§6 currently *excludes* "n8n deployment or any n8n integration", and owner
decision `O7` keeps n8n outside Phase 2. Nothing in this document authorises an
implementation; it exists so that the owner can accept, amend or reject a
concrete, bounded proposal instead of an open-ended idea. Until the status is
`Accepted`, no automation code, table or endpoint described here may be built.

---

## Context

The human's current direction names workflow automation as an intended ATOM
capability: *n8n for workflow orchestration and external integrations*, with
*laptop price monitoring as the first practical automation use case*. The
recorded product truth points the other way:

- [`../project-truth.md`](../project-truth.md) §6: "n8n deployment or any n8n
  integration" is deliberately excluded; §5 keeps n8n on the roadmap as
  `Proposed` behind owner decision `O7`; [`../roadmap.md`](../roadmap.md) §3
  describes it as "a separate Node service that calls the API; the API never
  depends on it and it never sits inside the data path of a private record".
- The codebase confirms the exclusion structurally: no HTTP client dependency
  exists in `Cargo.toml`, no webhook or callback route exists in
  `src/api/routes.rs`, and the only background job in the process is the
  session-prune loop (`src/main.rs`).
- The machine credential that automation would use already exists and is
  verified: scoped API keys (`migrations/0007`, `src/api_keys/`,
  [`ADR-0011`](ADR-0011-api-keys.md)) — a workflow can hold a revocable,
  owner-scoped credential today.
- The first automation use case (laptop price monitoring) needs four things
  ATOM does not have: scheduled collection, outbound fetches, a history of
  observations, and alerting. ATOM deliberately has no outbound client; n8n
  would supply scheduling and fetching, ATOM would own the data and the rules.

The cost of doing nothing: the automation direction stalls behind an
unresolved exclusion, and the first use case (price monitoring) has nowhere to
land.

## Decision

*(Proposed.)* ATOM treats automation as an **external orchestration boundary**:

1. **n8n stays a separate service.** It handles scheduling, outbound
   collection from approved sources, and notifications. It is never embedded
   in the API process, never added to the Rust dependency graph, and never
   deployed by the ATOM repository (the §6 exclusion of *deployment* is kept
   even if the *integration* exclusion is amended).
2. **ATOM owns the data and the rules.** PostgreSQL remains the source of
   truth for tracked products, observations, rules and alerts. n8n writes only
   through authenticated ATOM endpoints; it never receives a database URL, and
   it can never bypass ownership checks, because every write is an ordinary
   authenticated request evaluated by the existing service layer.
3. **The credential is an ATOM API key** (`ADR-0011`): `atom_` + 32 CSPRNG
   bytes, stored as a digest, revocable by the owner in the existing UI/API.
   A dedicated key is issued to the n8n instance; its `last_used_at` and
   revocation give the owner instant off-switch and an audit trail.
4. **Webhooks and callbacks are authenticated requests**, not a new trust
   boundary: any endpoint n8n calls requires the `Authorization` header exactly
   like every other private route, and the request body is validated by the
   same shared validators.
5. **Collection endpoints are idempotent.** Every write that a retry could
   duplicate carries a client-supplied idempotency identity (the observation
   fingerprint: product, listing, observed price, collected-at) so a retried
   workflow cannot double-write. Duplicates are refused or absorbed, never
   double-recorded.
6. **Failures are data.** A failed collection attempt is recorded as an
   execution outcome (not a silent gap), stale listings are marked rather than
   re-reported as current, and every alert records the observation it came
   from, so a repeated workflow run cannot raise a duplicate alert.
7. **The first use case is laptop price monitoring**, with the minimal schema
   below. Everything else on the automation list waits until this one is
   `Verified`.

### The minimal data model for the first use case

*(To be implemented as one forward-only migration, following
[`../database/migration-policy.md`](../database/migration-policy.md), only after
this ADR is accepted.)*

| Table | Purpose | Key constraints |
| ----- | ------- | ---------------- |
| `tracked_products` | One row per product configuration the owner tracks (model, spec distinguishing RAM/storage/GPU/CPU/screen, normalised name). | `user_id` FK CASCADE; `UNIQUE (user_id, normalizedName, configHash)`; `status` (`active`, `paused`) with a CHECK. |
| `product_listings` | One row per retailer listing of a tracked product (retailer, URL, source attribution). | `user_id` FK CASCADE; FK to `tracked_products` (composite with `user_id`, per the `0004`/`0005` pattern); `UNIQUE (user_id, url)`; URL shape CHECK. |
| `price_observations` | One row per collected observation: amount `NUMERIC(19,4)` (`ADR-0007`, never float), currency `CHAR(3)`, collected-at, source, and a **fingerprint** for idempotency. | `user_id` FK CASCADE; FK to `product_listings` (composite); `UNIQUE (user_id, listing_id, fingerprint)` — a retried run is absorbed; amount CHECK `>= 0`; index `(user_id, listing_id, collected_at DESC)` for history reads. |
| `monitoring_rules` | Target-price rules: one rule per product (or listing), a target `NUMERIC(19,4)`, an enabled flag. | `user_id` FK CASCADE; FK to `tracked_products` (composite); `target_amount` CHECK `>= 0`. |
| `automation_runs` | One row per n8n execution that touches ATOM: started/finished, outcome (`ok`/`partial`/`failed`), an error summary, and the counts it reported. | `user_id` FK CASCADE; outcome CHECK; index `(user_id, started_at DESC)`. |
| `price_alerts` | One row per alert raised: the rule, the observation that triggered it, created-at. | `user_id` FK CASCADE; FKs to `monitoring_rules` and `price_observations` (both composite); `UNIQUE (rule_id, observation_id)` — duplicate alerts are unrepresentable. |

Collection rules (bound by this ADR if accepted): official APIs or permitted
feeds first; the terms, robots policy and rate limits of each source are
respected; no authentication, CAPTCHA or anti-bot mechanism is bypassed; a
source that does not permit automated collection is not collected; suspicious
or missing prices are recorded as failures, never as current prices; normalised
product identity is deterministic first (exact model + config hash), and any
assisted matching is a separate, validated decision.

## Alternatives considered

| Alternative | Why it was not chosen |
| ----------- | --------------------- |
| Build scheduling, fetching and alerting inside the Rust process (cron loop + a Rust HTTP client). | Violates the accepted local-only, no-second-runtime boundary in spirit and letter at the wrong layer: it adds an outbound HTTP client, a scheduler and retry/queue state to the API process that the audit found deliberately free of them. It also puts scraping policy inside the product code the owner cannot adjust without a release. n8n gives scheduling, retries and credentials externally, which is exactly its job. |
| n8n writes directly to PostgreSQL with its own credentials. | Bypasses ownership enforcement (ADR-0005 is enforced in SQL against `user_id` from the authenticated principal, not from a shared automation role), bypasses validation, and defeats the error contract. Explicitly rejected. |
| A second Rust worker service (microservice) for automation. | Contradicts [`ADR-0001`](ADR-0001-modular-monolith.md) and the §6 exclusion of microservices; the orchestration problem (external feeds, schedules, notifications) is not one Rust is needed for, and the API gains nothing from owning it. |
| Do the first use case as a throwaway script beside the repository. | The human's instruction forbids it, and it would lose ownership, history, alerting and the audit trail — the four things that make monitoring data trustworthy. |
| Keep the exclusion and drop automation entirely. | A legitimate owner option — this ADR does not presume the answer. If the owner rejects it, the §6 exclusion stands and the price-monitoring direction stops; the cost is the one named in *Context*. |

## Consequences and trade-offs

### Positive

- The API's security model is untouched: no new trust boundary, no new
  credential type, no database credential shared with an external tool.
- The existing, `Verified` API-key machinery is exactly the integration
  credential; no backend change is required for authentication.
- The owner can pause or revoke all automation instantly (revoke the key).
- The price-monitoring schema follows the ownership and money conventions the
  codebase already enforces and tests (`ADR-0005`, `ADR-0007`, the composite-FK
  pattern of `0004`/`0005`).
- Failures and staleness become queryable data instead of a silent absence.

### Negative

- A second runtime (n8n) enters the deployment, even though it stays out of
  the API process — the owner accepts an operational surface the current
  exclusions deliberately avoid.
- Outbound collection inherits the legal and policy posture of its sources;
  the per-source allowlist is a standing maintenance obligation, not a
  one-time decision.
- Webhook/collection endpoints expand the public surface: every one must be
  tested for credential enforcement and idempotency like the rest (the
  `contract_sweep` pattern).
- The `embeddings`-style polymorphic problem repeats if observations later
  attach to other entities; the schema above keeps every edge a real composite
  foreign key to avoid that.
- n8n's own execution history lives outside ATOM; `automation_runs` records
  the ATOM-side outcome only (bounded scope, stated rather than hidden).

## Security and operational implications

- **Trust boundary:** unchanged inside ATOM. n8n is an authenticated client,
  not a trusted component; its credential is revocable and owner-scoped, and
  its requests pass through the same extractors, validators and ownership
  predicates as every other client.
- **Secrets:** the n8n instance holds its ATOM API key and any source
  credentials in n8n's own secret store — never in this repository, never in
  `config/`, never in an ATOM table.
- **Rate limiting:** collection endpoints should be covered by the rate-limit
  decision for authenticated routes (the audit's P-8); if per-user limits are
  adopted for `/api-keys` and `/ml/reindex`, the same mechanism covers the new
  endpoints. **Monitoring-write rate limiting is a recorded production
  security decision** (independent review 2026-10-31, finding I-4): the
  endpoints carry no per-user write limit of their own — the same posture as
  every existing feature — and the acceptance of a machine credential that
  writes on a schedule makes the standing edge rate-limiting control the
  production boundary for this traffic. The limiter's own open questions
  (fail-open vs fail-closed, the trusted-proxy declaration that decides what
  an "edge" is) are `Q-09` and `Q-07` in
  [`../engineering/open-questions.md`](../engineering/open-questions.md);
  they must be answered before a production deployment of the workflow.
- **Abuse containment:** a compromised workflow key sees and writes only what
  its owner sees and writes; revoking it ends the access. `automation_runs`
  gives the owner an audit trail of what the workflow did and when.
- **Operations:** the operator runs and upgrades n8n separately; ATOM's build
  and start-up remain free of it (the "build must not require a hosted or paid
  service" constraint in `project-truth.md` §8 is kept for the API itself;
  n8n is an *external* deployment the owner chooses to operate).

## Related requirements and changes

| Reference | Relationship |
| --------- | ------------ |
| [`../project-truth.md`](../project-truth.md) §6, §6.1 | The exclusion this ADR asks the owner to amend (integration, not deployment). |
| [`../roadmap.md`](../roadmap.md) §3, decision `O7` | The roadmap row this ADR would let move from `Proposed`. |
| [`ADR-0011-api-keys.md`](ADR-0011-api-keys.md) | The credential automation would hold. |
| [`ADR-0005-ownership-enforced-in-sql.md`](ADR-0005-ownership-enforced-in-sql.md) | Why direct database writes from n8n are rejected. |
| [`ADR-0007-money-representation.md`](ADR-0007-money-representation.md) | The money representation the price tables must follow. |
| [`../database/migration-policy.md`](../database/migration-policy.md) | The migration rules the new tables would follow (forward-only). |

## Supersedes / Superseded by

`Supersedes:` none — this is the first ADR on automation scope; it does not
replace an earlier decision, it proposes the first one.

`Superseded by:` none. A review of this decision is due when the requirement
it serves changes.
