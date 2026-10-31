# Data sources — verification checklist

> **Status:** Proposed — every row is unverified
> **Last updated:** 2026-10-31
> **Owner:** maintained by the `docs` agent (content owned by the price-monitoring workstream)

The gate every collection source must pass before the workflow's fetch node
may touch it. Until a row says otherwise, **no source is verified and no
endpoint is asserted** — no retailer, no URL, nothing. This is the
[`ADR-0012`](../../decisions/ADR-0012-automation-orchestration-scope.md)
collection rule made operational: official APIs or permitted feeds first;
terms, robots policy and rate limits respected; no authentication, CAPTCHA
or anti-bot mechanism bypassed; **a source that does not permit automated
collection is not collected**.

## 1. The checklist

| # | Source | Terms check | API / feed | Rate limit | Status |
| - | ------ | ----------- | ---------- | ---------- | ------ |
| 1 | *(to be named — official API candidate)* | not done | unknown | not measured | **unverified** |
| 2 | *(to be named — permitted price feed candidate)* | not done | unknown | not measured | **unverified** |
| 3 | *(to be named — public page candidate)* | not done | n/a — would need full robots + terms review | not measured | **unverified** |

There are no other rows. A row is added only when a candidate source has a
name, and it enters as **unverified**.

## 2. What promotes a row to verified

All of the following, recorded on the row with the date and the evidence:

1. **Terms permit automated collection** for this exact use — read, not
   assumed; the location of the permission (or the written permission
   itself) is cited.
2. **Robots policy respected** for the paths actually fetched, where
   applicable.
3. **No bypass**: nothing behind authentication, CAPTCHA or an anti-bot
   mechanism is collected; a source that requires any of those to fetch is
   out, permanently, whatever its terms say elsewhere.
4. **The transport is an official API or a permitted feed** — preferred —
   or a page fetch whose terms explicitly allow it.
5. **The rate limit is measured and recorded**, and the workflow's cadence
   is set below it with headroom.
6. **The endpoint, its authentication and its price format** are captured
   in the workflow design ([`laptop-price-monitoring.md`](laptop-price-monitoring.md)),
   whose fetch node stays disabled until then.

A source that fails any check gets the row closed with the reason — a
recorded "no" protects the project from re-litigating it later. A source
with no terms at all is treated as **not permitting** automated collection.

## 3. Related documents

- [`laptop-price-monitoring.md`](laptop-price-monitoring.md) — the workflow this gate protects.
- [`../integration-contract.md`](../integration-contract.md) §9.4 — the hard boundary this checklist enforces.
- [`../../decisions/ADR-0012-automation-orchestration-scope.md`](../../decisions/ADR-0012-automation-orchestration-scope.md) — the collection rules.
