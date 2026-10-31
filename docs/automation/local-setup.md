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

## 6. Related documents

- [`integration-contract.md`](integration-contract.md) — the contract the local setup must honour.
- [`workflows/laptop-price-monitoring.md`](workflows/laptop-price-monitoring.md) — the importable skeleton.
- [`../decisions/ADR-0012-automation-orchestration-scope.md`](../decisions/ADR-0012-automation-orchestration-scope.md) — the accepted boundary; deployment excluded.
- [`../decisions/ADR-0011-api-keys.md`](../decisions/ADR-0011-api-keys.md) and [`../features/api-keys/status.md`](../features/api-keys/status.md) — the credential.
