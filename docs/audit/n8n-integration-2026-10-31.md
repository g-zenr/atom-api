# ATOM × n8n — local integration report

> **Status:** Point-in-time execution record (uncommitted, per the milestone's Git rules)
> **Date:** 2026-10-31
> **Scope:** local n8n integration against the verified price-monitoring domain
> **Verdict:** **INTEGRATION VERIFIED WITH CONDITIONS**
> **Companion docs:** [`../automation/local-setup.md`](../automation/local-setup.md),
> [`../automation/workflows/laptop-price-monitoring.md`](../automation/workflows/laptop-price-monitoring.md),
> [`../automation/integration-contract.md`](../automation/integration-contract.md)

No retailer was contacted. The workflow's fetch node stayed `disabled: true`;
prices were synthesized from dev-only variables.

---

## 1. Environment inspection

| Item | Finding |
| ---- | ------- |
| `atom-api` working tree | clean, `develop` in sync with `origin/develop` |
| `atom-web` working tree | clean, `main` in sync with `origin/main` |
| Backend | `/health/ready` → `200` `{"status":"ready","checks":{"database":"ok"}}` |
| PostgreSQL | `atom-postgres-dev` (`postgres:18-alpine`) up; disposable `atom_dev` |
| Docker | engine 29.2.1 (Docker Desktop had to be started) |
| Existing n8n | none for this project; the only n8n container was created by this work |
| Pre-existing containers | `atom-postgres-dev` (kept), `atom-shutdown-db` (exited, untouched) |

**Safety note:** the `n8n-mcp` server in this session points at a **remote**
instance (`https://n8n.34-66-83-76.nip.io`) — a different, live n8n. It was
**not** used to create, update, run or delete anything; all work was local.

---

## 2. Docker / n8n setup

- **Image (pinned):** `n8nio/n8n:1.98.1`
- **Container:** `atom-n8n`, bound `127.0.0.1:5678` only (never `0.0.0.0`)
- **Volume:** `atom_n8n_data` → `/home/node/.n8n` (persists workflow + credential)
- **Env of note:** `ATOM_API_URL=http://host.docker.internal:3000`,
  `NODE_FUNCTION_ALLOW_BUILTIN=crypto`, `N8N_ENCRYPTION_KEY=dev-only-change-me`,
  `GENERIC_TIMEZONE=Asia/Manila`
- **Health:** `docker logs` reached `n8n ready on ::, port 5678`; `/healthz` → `200`
- **After the milestone:** container **stopped and removed**; volume **retained**
  (resume = re-run the documented `docker run`, §6.2 of `local-setup.md`)

The owner account was set up previously by this integration work; its
password was reset through the local SQLite (bcrypt, 10 rounds) so the
internal REST API could be driven headlessly. The password is **not**
recorded here or in any committed file.

---

## 3. Credential status (no secrets)

| Credential | Where | State |
| ---------- | ----- | ----- |
| `ATOM API key` (Header Auth) | n8n credential store, id `ahPWCtp9zaj5DgBx` | holds `Authorization: Bearer atom_…`; **encrypted at rest**, never in the workflow |
| curl-test key | minted for the negative tests | **revoked** at the end (DELETE → `204`) |

The key never appears in a node parameter, an expression, an export, or any
execution record. A scan of all 14 n8n execution rows plus the workflow rows
for `atom_*`, `Bearer atom_`, and the owner/test passwords returned **0 hits**.

---

## 4. API operations exercised

| Operation | Use |
| --------- | --- |
| `POST /api/v1/auth/register`, `POST /api/v1/auth/login` | test owner set-up (fixtures) |
| `POST /api/v1/products` | tracked product ("Test Gaming Laptop") |
| `POST /api/v1/products/{product_id}/listings` | listing ("Synthetic Test Store", PHP) |
| `POST /api/v1/products/{product_id}/rules` | target-price rule `45000.0000` PHP |
| `POST /api/v1/api-keys`, `DELETE /api/v1/api-keys/{key_id}` | mint / revoke the disposable key |
| `POST /api/v1/automation/runs` | start a run (workflow *Start run*) |
| `GET /api/v1/products?status=active&limit=…` | paginated active products (fan-out) |
| `GET /api/v1/products/{product_id}/listings` | per-product listings (fan-out) |
| `POST /api/v1/listings/{listing_id}/observations` | the idempotent write |
| `GET /api/v1/alerts?limit=…` | aggregate the run's alert count |
| `PATCH /api/v1/automation/runs/{run_id}` | finish the run (*Patch run finish*) |

Every path was read from `src/api/routes.rs`; none was invented.

---

## 5. Workflow node list (16 nodes, as run)

1. Schedule Trigger (only entry point)
2. Start run (`POST /automation/runs`)
3. Get active products (`GET /products`, paginated)
4. Split out products
5. Loop over products
6. Get product listings (`GET /products/{id}/listings`)
7. Split out listings
8. Loop over listings
9. Fetch listing source — **`disabled: true`** (no retailer)
10. Normalize and validate price (fingerprint + synthetic price)
11. Post observation (idempotent) (`POST /listings/{id}/observations`, `neverError`)
12. Verify absorption (contract §4 tuple comparison)
13. **Count run alerts** (`GET /alerts`) — added during integration
14. Patch run finish (`PATCH /automation/runs/{run_id}`)
15. Any failures?
16. Error path (notify owner) — `noOp` placeholder

---

## 6. Successful execution evidence

Fixture: synthetic PHP prices, no real source.

| n8n exec | ATOM run | Outcome | Observation | Alert |
| -------- | -------- | ------- | ----------- | ----- |
| 9 | `323a69ba-…8d70` | `ok` (obs 1, alerts 0) | `6eaab9bc` 50000.0000 @ 09:00 | none (50000 > 45000) ✓ |
| 10 | `95c4c980-…26e4` | `ok` (obs 1, alerts 0) | `54190ecb` 44000.0000 @ 10:00 | `9cb3136f` **fired** ✓ |
| 11 | `cec399df-…0e94` | `ok` (obs 1, **alerts 1**) | `c4cec1bd` 43000.0000 @ 11:00 | `3f853acb` **fired** ✓ |
| 12 | `18406f82-…5e09` | `ok` (obs 1, alerts 0) | absorbed (see §8) | none (duplicate) ✓ |

Exec 11 is the strongest single result: the observation was **created** (`201`),
the target rule **fired**, and the run's `alerts` count **reported 1** — proving
the *Count run alerts* aggregation.

---

## 7. Failure-scenario evidence

| Scenario | Result |
| -------- | ------ |
| Invalid API key | `401` |
| Revoked API key (minted → `DELETE` `204` → reuse) | `401` |
| Invalid price `"12.5"` | `422` |
| Invalid currency `"US"` | `422` |
| Fingerprint too long (250 chars) | `422` |
| Missing listing | `404` |
| Missing product | `404` |
| No credential | `401` |
| **Incorrect fingerprint → failed run** | exec **13**, run `284686df` → **`failed`**, `failures 1`, `error_summary "1 listing(s) failed collection"` |
| Observation submission failure (invalid price via workflow) | run `ef1139b5` → `failed`, 1 failure (same local instance) |
| Run-completion failure (finish twice) | `200` then **`409`** |
| ATOM API unavailable (n8n pointed at a dead port) | exec **14** status **`error`**; **no run created** (11 runs before, 11 after) — nothing left `running` |

### 7.1 The fingerprint-mismatch proof (exec 13, exec detail)

The workflow was temporarily pointed at a **constant** fingerprint (a
simulated buggy client) and a new tuple (`39000.0000 @ 13:00`) was submitted:

- `POST` answered **`200`** and returned the **existing** row `43000.0000 @ 11:00`
  (same fingerprint — absorbed).
- *Verify absorption* compared sent vs returned → `ok=false`
  (`amount`/`observed_at`/`listing`/`fingerprint` mismatch).
- The run finished **`failed`** with an actionable `error_summary` — **not a
  silent success**.

The injected constant was then reverted; the restored *Normalize* node again
builds `sha256(product|listing|price|observed_at)`.

---

## 8. Duplicate / idempotency evidence

Exec 11 and exec 12 sent the **identical** tuple (`43000.0000 @ 11:00`, same
fingerprint `sha256:cab7d2…`):

| | exec 11 | exec 12 |
| - | ------- | ------- |
| `POST /observations` status | **201** | **200** |
| returned `observation_id` | `c4cec1bd-…` | `c4cec1bd-…` (same row) |
| *Verify absorption* | `ok=true`, `absorbed=false` | `ok=true`, **`absorbed=true`** |
| run `observations` / `alerts` | 1 / 1 | 1 / 0 |

**PostgreSQL confirmed exactly one row** for that fingerprint: the listing held
**5 observations** and the rule **3 alerts** after both runs — no duplicate
row, no duplicate alert.

---

## 9. PostgreSQL state verification (read-only)

Product `44be4536` ("Test Gaming Laptop"), listing `633206a2`:

| amount | observed_at | run outcome | alert |
| ------ | ----------- | ----------- | ----- |
| 46000.0000 | 06:00 | failed (prior) | – |
| 44000.0000 | 07:00 | ok | ✓ |
| 50000.0000 | 09:00 | ok | – |
| 44000.0000 | 10:00 | ok | ✓ |
| 43000.0000 | 11:00 | ok | ✓ |

Price history is monotonic by `observed_at`, no row overwritten, alerts
deduplicated by `(rule_id, observation_id)`. All reads were `SELECT`-only
against `atom_dev`; n8n itself has **no** database access.

---

## 10. Remaining issues

1. **`alerts` count is approximate by design.** `POST /observations` does not
   return alert information and `GET /alerts` has no run filter, so the
   workflow counts alerts with `triggered_at >= run.started_at` (correct for a
   non-overlapping schedule). ATOM's `price_alerts` is authoritative.
2. **Product with no listing.** *Verify absorption* would not execute and
   *Patch run finish* references it; needs a guard before a schedule is enabled.
3. **Prior-run artifacts.** Four early executions (1–4) errored while the
   finish-node wiring was being fixed, and one run (`bf9bd7fa`) was closed
   manually — historical, in `atom_dev` only.
4. **Not exercised:** browser E2E; a real retailer source (all rows in
   `data-sources.md` remain unverified); notifications (the *Error path* node is
   a `noOp` placeholder).
5. **Deprecation noise:** the pinned image warns about `N8N_RUNNERS_ENABLED`
   and settings-file permissions — dev-only.

---

## 11. Files changed (all uncommitted)

- `docs/automation/local-setup.md` — added §6 "The local integration, as tested"
  (env, start/stop, headless import/run, the synthetic-price harness, history
  inspection, revocation, troubleshooting, limitations).
- `docs/automation/workflows/laptop-price-monitoring.md` — added the *Count run
  alerts* node to the table and the importable JSON (16 nodes / 17 connection
  targets), and refreshed the finish-body note.
- `docs/audit/n8n-integration-2026-10-31.md` — this report.
- The n8n-side workflow lives in the retained volume, not the repository.

No commit or push was made; `atom-web` is untouched.

---

## 12. Recommended next milestone

**Local, scheduled, source-verified collection.** Add a single verified
data-source row (official API or permitted feed), wire the fetch node to it
with the placeholder filled, add the no-listing guard, and run it on a
schedule against disposable data — then consider the notification channel.
Production deployment, real retailer accounts, and credential changes remain
out of scope and separately authorised.

---

## Final verdict

**INTEGRATION VERIFIED WITH CONDITIONS.** End-to-end execution, idempotent
absorption, rule firing, alerting, the absorption-verification guard, and the
failure/negative scenarios all behaved correctly and are backed by n8n
execution ids and PostgreSQL rows. The conditions are the honest limits above
(approximate `alerts` count, no-listing guard, no retailer source, no browser
E2E) — none of which blocks the next milestone.

---

### Delivery context

- **Orchestrator / execution:** `nvidia/deepseek-ai/deepseek-v4.1-flash`
  (OpenCode, metered) — the delegated builder subagent failed with an HTTP 429
  from the provider, so the orchestrator executed the milestone directly with
  no silent model fallback.
- **n8n worker:** `n8nio/n8n:1.98.1` (local container).
- **Evidence is reproducible** from §6.2–§6.4 of `local-setup.md` plus the
  PostgreSQL snapshots above.
