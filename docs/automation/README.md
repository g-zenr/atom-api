# ATOM automation — the n8n integration

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-31
> **Owner:** maintained by the `docs` agent

This directory documents how an external n8n instance automates work against
ATOM. The boundary it records is accepted:
[`../decisions/ADR-0012-automation-orchestration-scope.md`](../decisions/ADR-0012-automation-orchestration-scope.md)
was accepted on 2026-10-31 by owner decision `O7`, after being proposed on
2026-10-30. The authority order of
[`../README.md`](../README.md) §1 applies: the accepted ADR and
[`../project-truth.md`](../project-truth.md) govern this directory, not the
other way round.

## 1. The boundary in one paragraph

n8n is an **external orchestrator**, not a trusted component. It schedules
work, collects from approved sources and notifies; ATOM owns the data and the
rules. Every write n8n makes is an ordinary authenticated API request carrying
a dedicated, revocable API key — the same extractors, validators and
ownership predicates evaluate it as they would any other client. n8n never
receives a database credential and can never bypass
[`ADR-0005`](../decisions/ADR-0005-ownership-enforced-in-sql.md). Deploying
n8n remains **outside** this repository: the ADR amends the *integration*
exclusion only, and a real deployment is a separate owner decision.

## 2. The documents

| Document | Status | Contents |
| -------- | ------ | -------- |
| [`integration-contract.md`](integration-contract.md) | Accepted | The complete n8n↔ATOM contract: authentication, the endpoint surface, idempotency, retryable and non-retryable failures, duplicate-event handling, the run outcome model, recovery procedures and the hard boundaries. |
| [`local-setup.md`](local-setup.md) | Accepted | Running n8n locally for development: the runtime options, the ATOM URL and where the API key lives. **Deployment needs owner approval.** |
| [`workflows/`](workflows/) | Proposed | The workflow designs and the data-source verification checklist. |

### Under [`workflows/`](workflows/)

| Document | Status | Contents |
| -------- | ------ | -------- |
| [`workflows/laptop-price-monitoring.md`](workflows/laptop-price-monitoring.md) | Proposed | The initial workflow as an importable JSON skeleton. Placeholders only — **no retailer endpoint is asserted**; each source is verified before integration. |
| [`workflows/data-sources.md`](workflows/data-sources.md) | Proposed | The source checklist. **Every row is unverified** until its terms, feed and rate limit are checked and recorded. |

## 3. The hard boundaries, in short

1. **No direct database access.** n8n never holds `DATABASE_URL` or any
   PostgreSQL credential; every write is an authenticated API request.
2. **No business logic in n8n.** Target-price rules, alert decisions and
   ownership stay in ATOM. The workflow does scheduling, transport hygiene
   and retries.
3. **No secrets in workflow logs.** The API key lives in n8n's credential
   store; it is never in a node parameter, an expression, an export or a log
   line — and ATOM never logs it either.
4. **No unverified source.** A source is collected only after its terms,
   robots policy and rate limits are checked and recorded in
   [`workflows/data-sources.md`](workflows/data-sources.md). No
   authentication, CAPTCHA or anti-bot mechanism is ever bypassed.

The full statement of each boundary, with its rationale, is
[`integration-contract.md`](integration-contract.md) §9.

## 4. Related documents

- [`../decisions/ADR-0012-automation-orchestration-scope.md`](../decisions/ADR-0012-automation-orchestration-scope.md) — the accepted decision this directory implements.
- [`../decisions/ADR-0011-api-keys.md`](../decisions/ADR-0011-api-keys.md) — the credential a workflow holds.
- [`../features/api-keys/status.md`](../features/api-keys/status.md) — the verified API-key operations the integration uses.
- [`../roadmap.md`](../roadmap.md) §3 — the `O7` decision record.
- [`../project-truth.md`](../project-truth.md) — the product requirements the automation serves.
