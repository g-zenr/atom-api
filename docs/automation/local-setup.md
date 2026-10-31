# Running n8n locally for ATOM development

> **Status:** Accepted
> **Last updated:** 2026-10-31
> **Owner:** maintained by the `docs` agent

How to run a **development** n8n alongside a local ATOM API. Everything here
is for local development against `127.0.0.1`.

> **Deployment needs owner approval.** Standing n8n up anywhere beyond a
> developer machine — a server, a container host, a managed n8n — is a real
> deployment and is **not** authorised by the accepted
> [`../decisions/ADR-0012-automation-orchestration-scope.md`](../decisions/ADR-0012-automation-orchestration-scope.md):
> the ADR amends the *integration* exclusion only and explicitly keeps the
> *deployment* exclusion in force. This repository never deploys, configures
> for production, or pays for n8n.

## 1. Prerequisites

- The ATOM API running locally: `cargo run` serves `127.0.0.1:3000`, with
  migrations applied and an owner account registered.
- Node.js 18+ (for option A) or Docker (for option B).
- An owner access token, to mint the API key (§3).

## 2. Start n8n

Pick one. Both keep all state inside the development machine.

**Option A — npx (quickest):**

```bash
npx n8n
```

n8n serves `http://localhost:5678`. Data lives in `~/.n8n`.

**Option B — docker compose (throwaway, dev-only):**

```yaml
# docker-compose.dev-n8n.yml — development only, never deployed by this repo
services:
  n8n:
    image: docker.n8n.io/n8nio/n8n
    ports:
      - "5678:5678"
    environment:
      - N8N_SECURE_COOKIE=false        # http://localhost only — never a real deployment
      - N8N_ENCRYPTION_KEY=dev-only-change-me
      - GENERIC_TIMEZONE=UTC
      - ATOM_API_URL=http://host.docker.internal:3000
    volumes:
      - n8n_dev_data:/home/node/.n8n
volumes:
  n8n_dev_data:
```

```bash
docker compose -f docker-compose.dev-n8n.yml up
```

The `N8N_ENCRYPTION_KEY` here is a **development-only** placeholder that
encrypts the local credential store; a real deployment would generate its
own and keep it secret. The compose file is an example for the developer's
machine — it is not part of the repository's runtime and must not be
committed with real values.

## 3. Point n8n at ATOM

1. **The base URL** is supplied to the workflow through the
   `ATOM_API_URL` environment variable of the *n8n process* —
   `http://127.0.0.1:3000` for option A, `http://host.docker.internal:3000`
   for option B — and referenced inside nodes as
   `{{ $env.ATOM_API_URL }}`. No node ever hard-codes a URL.
2. **The API key** lives in **n8n's credential store**, never in the
   workflow:
   - Mint it first from your owner session (the key is returned once):

     ```bash
     curl -X POST http://127.0.0.1:3000/api/v1/api-keys \
       -H "Authorization: Bearer $ATOM_OWNER_JWT" \
       -H "Content-Type: application/json" \
       -d '{"name": "n8n"}'
     ```

     The response `key` field is the only time the secret exists outside
     n8n. The full operation contract is
     [`../features/api-keys/status.md`](../features/api-keys/status.md).
   - In n8n: **Credentials → New → Header Auth**, with name `Authorization`
     and value `Bearer atom_…`. The workflow's HTTP Request nodes reference
     this credential (generic credential type, header auth) — they contain
     a reference, not the secret.
   - Never paste the key into a node parameter, an expression, a code node
     or a workflow export. The integration contract's §9.3 makes this
     binding: [`integration-contract.md`](integration-contract.md) §9.3.
3. **Rotation** (dev included): mint the new key, switch the credential,
   revoke the old one — in that order, per
   [`integration-contract.md`](integration-contract.md) §8.3.

## 4. First-run checklist

1. Import the skeleton from
   [`workflows/laptop-price-monitoring.md`](workflows/laptop-price-monitoring.md).
2. Leave the workflow **inactive** — it ships inactive, with the fetch node
   disabled.
3. Fill placeholders only for a source recorded as verified in
   [`workflows/data-sources.md`](workflows/data-sources.md). If no source is
   verified yet, nothing runs against a retailer — that is the point of the
   gate.
4. Run it manually against the local ATOM, confirm the run lifecycle and
   idempotent absorption behave as
   [`integration-contract.md`](integration-contract.md) §4 and §7 describe,
   before ever enabling a schedule.

## 5. What not to do locally

- **No database credentials in n8n.** Not for convenience, not for
  debugging — every write goes through the API
  ([`integration-contract.md`](integration-contract.md) §9.1).
- **No business logic in the workflow.** If a decision about what the data
  means wants to live in n8n, it belongs in ATOM (§9.2).
- **No secrets in exports or logs.** A shared workflow JSON must be
  importable without leaking anything (§9.3).
- **No unverified source fetch.** Not even once, "just to see"
  (§9.4).
- **No production use of this setup.** The moment n8n runs anywhere but
  your machine, it is a deployment — owner approval first.

## 6. The local integration, as tested (2026-10-31)

This section records the **exact** procedure that was executed on a Windows
development machine on 2026-10-31, with the pinned image and the observed
results. Anything not in this section was not exercised.

### 6.1 Environment actually used

| Item | Value |
| ---- | ----- |
| n8n image (pinned) | `n8nio/n8n:1.98.1` |
| Container name | `atom-n8n` (local-only; the operator's own container) |
| Data volume | `atom_n8n_data` → `/home/node/.n8n` (persists the workflow + credential) |
| Bind | `127.0.0.1:5678` → `5678` (never `0.0.0.0`) |
| API base seen by n8n | `http://host.docker.internal:3000` |
| Workflow id in n8n | `Uy3KrXCkRh1N2DvD` ("Laptop price monitoring (ATOM local integration)", inactive) |
| Credential | one **Header Auth** credential named `ATOM API key`, holding `Authorization: Bearer atom_…` |
| ATOM API | `cargo run --release --bin atom-api` against the disposable dev database |
| Timezone | `GENERIC_TIMEZONE=Asia/Manila` |
| Code-node builtins | `NODE_FUNCTION_ALLOW_BUILTIN=crypto` (the fingerprint uses `crypto`) |

### 6.2 Start / stop n8n (tested command)

```bash
docker run -d --name atom-n8n \
  -p 127.0.0.1:5678:5678 \
  -v atom_n8n_data:/home/node/.n8n \
  -e N8N_SECURE_COOKIE=false \
  -e N8N_ENCRYPTION_KEY=dev-only-change-me \
  -e NODE_FUNCTION_ALLOW_BUILTIN=crypto \
  -e GENERIC_TIMEZONE=Asia/Manila \
  -e ATOM_API_URL=http://host.docker.internal:3000 \
  n8nio/n8n:1.98.1
```

`docker logs atom-n8n` was checked until `n8n ready on ::, port 5678`, then
`http://127.0.0.1:5678/healthz` until it answered `200`. Stop with
`docker stop atom-n8n`; to change an environment variable the container must
be recreated (`docker rm -f atom-n8n`, then the command above). The **named
volume `atom_n8n_data` keeps the workflow and the credential across a
recreate**, so resuming is: run the `docker run` command above again (a
removed container cannot be brought back with `docker start`). The
integration milestone left the container **stopped and removed, volume
retained**.

### 6.3 Import the workflow

**UI:** *Workflows → Import from File/Clipboard* and paste the JSON from
[`workflows/laptop-price-monitoring.md`](workflows/laptop-price-monitoring.md)
§2.

**Headless (what was actually used):** the internal REST API, after an owner
session:

```bash
# log in (owner) -> n8n-auth cookie
curl -c cookies.txt -X POST http://127.0.0.1:5678/rest/login \
  -H "Content-Type: application/json" \
  -d '{"emailOrLdapLoginId":"<owner-email>","password":"<owner-password>"}'
# update the stored workflow (nodes + connections)
curl -b cookies.txt -X PATCH http://127.0.0.1:5678/rest/workflows/<workflow-id> \
  -H "Content-Type: application/json" --data-binary @workflow.json
```

`n8n import:workflow --input=…` / `n8n export:workflow --id=…` also work from
inside the container and were used to snapshot the workflow.

### 6.4 Execute it

The workflow's only entry point is the **Schedule Trigger**, so the n8n CLI
`n8n execute --id=…` **cannot** start it (`Missing node to start execution`).
Two supported ways to run it manually:

1. **UI** — open the workflow, *Execute workflow*.
2. **Headless (what was actually used)** — the same internal REST call the UI
   makes:

   ```bash
   curl -b cookies.txt -X POST http://127.0.0.1:5678/rest/workflows/<id>/run \
     -H "Content-Type: application/json" \
     -d '{"workflowData": <the workflow>, "runData": {}, "startNodes": []}'
   # -> {"data":{"executionId":"…"}}
   ```

### 6.5 The synthetic-price harness

No retailer is fetched (the fetch node ships `disabled: true`). The
*Normalize and validate price* Code node synthesizes the would-be collected
price from two **n8n-process** variables, so a run is fully controlled:

- `ATOM_TEST_PRICE` — the price string (must match `^[0-9]{1,15}\.[0-9]{4}$`).
- `ATOM_TEST_OBSERVED_AT` — the collected instant (RFC 3339 UTC).

A real collection replaces that block with the parsed output of a verified
source. Because the fingerprint is `sha256(product|listing|price|observed_at)`,
re-running with the **same** pair absorbs (`200`); changing either value
records a **new** observation (`201`).

### 6.6 Inspect the history

- **n8n side:** the *Executions* list in the UI, or
  `GET /rest/executions/<id>`. Note: n8n stores execution data in its
  internal *flatted* JSON form, so a raw parse is not human-readable; the UI
  is the practical view.
- **ATOM side:** `GET /api/v1/automation/runs`, `GET /api/v1/alerts`, and
  `GET /api/v1/listings/{listing_id}/observations`. Correlate the two by the
  `automation_runs.id` that the *Start run* response returns and every
  observation carries as `run_id`.

### 6.7 Revoke the disposable credential

```bash
curl -X DELETE http://127.0.0.1:3000/api/v1/api-keys/<key_id> \
  -H "Authorization: Bearer <owner-jwt>"
```

A revoked key answers `401` on the next call (verified). Rotation order is in
[`integration-contract.md`](integration-contract.md) §8.3: create the new key,
switch the n8n credential, then revoke the old one.

### 6.8 Troubleshooting (observed)

| Symptom | Cause | Fix |
| ------- | ----- | --- |
| `n8n execute --id=…` → `Missing node to start execution` | the workflow starts from a Schedule Trigger | run it from the UI or the `/rest/workflows/<id>/run` call (§6.4) |
| `n8n` logs `N8N_RUNNERS_ENABLED` deprecation | task runners not enabled | dev-only noise; set `N8N_RUNNERS_ENABLED=true` on a newer image |
| `Permissions 0644 … settings file … too wide` | volume file mode | dev-only warning; `N8N_ENFORCE_SETTINGS_FILE_PERMISSIONS=true` silences it |
| Every HTTP node `401` | the credential holds a revoked or foreign key | mint a new key (§3) and update the credential |
| A run reports `alerts: 0` while an alert exists | the workflow counts alerts whose `triggered_at >= run.started_at`; an absorbed duplicate raises none | expected — the alert is authoritative in ATOM's `price_alerts` |

### 6.9 Known limitations (honest)

- **Alerts are read-only to the workflow.** The observation `POST` response
  does **not** carry alert information, and `GET /api/v1/alerts` has no
  run-scoped filter, so the workflow counts alerts with
  `triggered_at >= run.started_at` (accurate for a non-overlapping schedule —
  contract §7). The `alerts` count is informational; ATOM's `price_alerts`
  is authoritative.
- **A product with no listing** leaves *Verify absorption* unexecuted, and the
  finish step references it; a run whose product set has no eligible listing
  needs the guard noted before enabling a schedule.
- **A confirmed fingerprint-construction failure** finishes the run `failed`
  with `error_summary` (verified), but the workflow does not retry — a retry is
  an operator decision.
- Browser E2E and a real retailer source were **not** exercised.

## 7. Related documents

- [`integration-contract.md`](integration-contract.md) — the contract the local setup must honour.
- [`workflows/laptop-price-monitoring.md`](workflows/laptop-price-monitoring.md) — the importable skeleton.
- [`../decisions/ADR-0012-automation-orchestration-scope.md`](../decisions/ADR-0012-automation-orchestration-scope.md) — the accepted boundary; deployment excluded.
- [`../decisions/ADR-0011-api-keys.md`](../decisions/ADR-0011-api-keys.md) and [`../features/api-keys/status.md`](../features/api-keys/status.md) — the credential.
