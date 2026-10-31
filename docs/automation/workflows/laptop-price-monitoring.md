# Workflow design — laptop price monitoring

> **Status:** Proposed
> **Last updated:** 2026-10-31
> **Owner:** maintained by the `docs` agent (content owned by the price-monitoring workstream)

The initial workflow design for the first automation use case, as an
importable n8n JSON skeleton. It implements the
[`../integration-contract.md`](../integration-contract.md) against the
boundary accepted in
[`../../decisions/ADR-0012-automation-orchestration-scope.md`](../../decisions/ADR-0012-automation-orchestration-scope.md).

**This design is a skeleton, not a built workflow.** Every node is a
placeholder to be filled **per verified source**. It asserts **no retailer
endpoint**: nothing here names or implies a fetchable URL, and the fetch
node ships disabled. Before any source is wired in, ADR-0012's collection
rules bind the integrator — official APIs or permitted feeds first; the
terms, robots policy and rate limits of each source are respected; no
authentication, CAPTCHA or anti-bot mechanism is bypassed; **a source that
does not permit automated collection is not collected**; suspicious or
missing prices are recorded as failures, never as current prices. The gate
is [`data-sources.md`](data-sources.md): no row there is verified yet, so
nothing may fetch yet.

## 1. The chain

The API has no cross-product listing enumeration: a listing belongs to a
product, and the observation is addressed by its listing alone. The chain
therefore fans out **products → listings → observations**, and opens the run
first so every observation it writes carries the run's id.

| Node (name) | Type | Role | Placeholder to fill |
| ----------- | ---- | ---- | ------------------- |
| Schedule Trigger | `n8n-nodes-base.scheduleTrigger` | The only scheduler. | The cadence — a per-source decision, set after the sources are verified. |
| Start run | `n8n-nodes-base.httpRequest` | `POST /api/v1/automation/runs` — opens the run (`201`, `running`); its id wires the finish step and every observation's `run_id`. | The credential reference. |
| Get active products | `n8n-nodes-base.httpRequest` | `GET /api/v1/products?status=active&limit=…`, paginating the shared `limit`/`offset` pair with the node's built-in pagination until `pagination.has_more` is false. | The page size (the `limit` and the offset multiplier must agree). |
| Split out products | `n8n-nodes-base.splitOut` | Turns the aggregated page envelopes into one item per product. | — |
| Loop over products | `n8n-nodes-base.splitInBatches` | One product at a time; the *done* output goes to the finish step. | Batch size, if a source's rate limit demands it. |
| Get product listings | `n8n-nodes-base.httpRequest` | `GET /api/v1/products/{product_id}/listings` for the current product. | — |
| Split out listings | `n8n-nodes-base.splitOut` | Turns the listing page into one item per listing. | — |
| Loop over listings | `n8n-nodes-base.splitInBatches` | One listing at a time; the *done* output returns to the product loop. | Batch size. |
| Fetch listing source | `n8n-nodes-base.httpRequest` | The outbound fetch. **Ships `disabled: true`.** | The per-source URL, its authentication and its robots/terms notes — only for a verified source. |
| Normalize and validate price | `n8n-nodes-base.code` | Transport hygiene: validate the price string, build the idempotency fingerprint, mark suspicious or missing prices as failures, and assemble the observation body (with the run id from *Start run*). | The parsing, per source. **Why Code:** item-level price-string validation and fingerprint construction are parsing no core node expresses; the alert rules themselves stay in ATOM. |
| Post observation (idempotent) | `n8n-nodes-base.httpRequest` | `POST /api/v1/listings/{listing_id}/observations` — `201` new, `200` absorbed, same body. `fullResponse` keeps the status code; `neverError` hands the `4xx` classification to the next node instead of failing the node. | Retry options per the contract §5 (the skeleton ships 3 tries). |
| Verify absorption | `n8n-nodes-base.code` | The contract §4 rule: a `200`/`201` is success **only when the returned row is the tuple that was sent** — `amount`, `observed_at`, `listing_id`, `fingerprint` compared; a mismatch is a fingerprint-construction failure, not a success. `401` stops the run; other `4xx` are item failures. | **Why Code:** the row-vs-sent tuple comparison across the three status classes is item-level verification no core node expresses. |
| Count run alerts | `n8n-nodes-base.httpRequest` | `GET /api/v1/alerts` — aggregates what the rules raised, so the finish body's `alerts` count reflects this run (the alerts whose `triggered_at` is at or after the run's `started_at`). | The page size, if the alert volume could exceed one page. |
| Patch run finish | `n8n-nodes-base.httpRequest` | `PATCH /api/v1/automation/runs/{run_id}` — closes the run opened by *Start run*: outcome, counts, error summary. | The outcome mapping (`ok` / `partial` / `failed`) and the counts. |
| Any failures? | `n8n-nodes-base.if` | True when the finished run reports failures. | The condition against the run summary. |
| Error path (notify owner) | `n8n-nodes-base.noOp` | Placeholder for the notification channel. It carries **no ATOM credential**. | The channel — a per-deployment decision. |

The run lifecycle follows the contract §7: the run opens as `running`
before collection, and the finish step records one of `ok`, `partial`,
`failed` — failures are data, carried in the run's error summary and
counts. A fingerprint-construction failure found by *Verify absorption*
is a failure too: the run finishes `failed` (or `partial`, when other
listings succeeded) with an actionable `error_summary`, never as a silent
success.

## 2. The skeleton (importable JSON)

Import via n8n (*Import from File/Clipboard*). The node `id`s are stable
placeholders; n8n regenerates what it needs on import. Structural
validation of the 15-node design with an n8n workflow validator recorded
**15 nodes, 15 connections, 0 errors, 1 warning** (2026-10-31); the one
warning is the intentional connection into the disabled fetch node, and
the parameter-level placeholders are listed in §3. The node *Count run
alerts* was added during the local integration
([`../local-setup.md`](../local-setup.md) §6), giving **16 nodes and 17
connection targets**, verified by a structural check (the JSON parses,
every connection endpoint resolves to a node, `id`s and names are
unique). The `splitInBatches`
outputs are the node's own convention: **output 0 is the *done* branch,
output 1 is the *loop* branch.**

```json
{
  "name": "Laptop price monitoring (skeleton)",
  "active": false,
  "settings": {
    "executionOrder": "v1",
    "saveDataErrorExecution": "all",
    "saveDataSuccessExecution": "all",
    "saveManualExecutions": true
  },
  "nodes": [
    {
      "id": "00000000-0000-0000-0000-000000000001",
      "name": "Schedule Trigger",
      "type": "n8n-nodes-base.scheduleTrigger",
      "typeVersion": 1.2,
      "position": [0, 0],
      "parameters": { "rule": { "interval": [ { "field": "cronExpression", "expression": "0 6 * * *" } ] } },
      "notes": "PLACEHOLDER cadence - a per-source decision, set only after the sources are verified."
    },
    {
      "id": "00000000-0000-0000-0000-000000000002",
      "name": "Start run",
      "type": "n8n-nodes-base.httpRequest",
      "typeVersion": 4.2,
      "position": [220, 0],
      "parameters": {
        "method": "POST",
        "url": "={{ $env.ATOM_API_URL }}/api/v1/automation/runs",
        "options": { "authentication": "genericCredentialType", "genericAuthType": "httpHeaderAuth" }
      },
      "credentials": { "httpHeaderAuth": { "id": "PLACEHOLDER-credential-id", "name": "ATOM API key" } },
      "notes": "Opens the automation_runs row: 201 with the run in its running state; the id is the run the finish step closes. A body, if any, is ignored by ATOM."
    },
    {
      "id": "00000000-0000-0000-0000-000000000003",
      "name": "Get active products",
      "type": "n8n-nodes-base.httpRequest",
      "typeVersion": 4.2,
      "position": [440, 0],
      "parameters": {
        "url": "={{ $env.ATOM_API_URL }}/api/v1/products",
        "sendQuery": true,
        "queryParameters": {
          "parameters": [
            { "name": "status", "value": "active" },
            { "name": "limit", "value": "50" }
          ]
        },
        "options": {
          "authentication": "genericCredentialType",
          "genericAuthType": "httpHeaderAuth",
          "pagination": {
            "pagination": {
              "paginationMode": "updateAParameterInEachRequest",
              "parameters": {
                "parameters": [ { "type": "qs", "name": "offset", "value": "={{ $pageCount * 50 }}" } ]
              },
              "paginationCompleteWhen": "other",
              "completeExpression": "={{ $json.pagination.has_more === false }}"
            }
          }
        }
      },
      "notes": "PLACEHOLDER page size - the limit here and the offset multiplier below must agree. The active filter and the shared limit/offset pair are the published contract; paused products are excluded here, which is what pausing means on the API side."
    },
    {
      "id": "00000000-0000-0000-0000-000000000004",
      "name": "Split out products",
      "type": "n8n-nodes-base.splitOut",
      "typeVersion": 1,
      "position": [660, 0],
      "parameters": { "fieldToSplitOut": "items", "options": {} },
      "notes": "ATOM answers one page envelope per response: { items, pagination }. This node turns the aggregated pages into one item per product."
    },
    {
      "id": "00000000-0000-0000-0000-000000000005",
      "name": "Loop over products",
      "type": "n8n-nodes-base.splitInBatches",
      "typeVersion": 3,
      "position": [880, 0],
      "parameters": { "options": {} },
      "notes": "One product at a time. Output 0 is the done branch (to the finish step); output 1 is the loop branch (to the listings call)."
    },
    {
      "id": "00000000-0000-0000-0000-000000000006",
      "name": "Get product listings",
      "type": "n8n-nodes-base.httpRequest",
      "typeVersion": 4.2,
      "position": [1100, -160],
      "parameters": {
        "url": "={{ $env.ATOM_API_URL }}/api/v1/products/{{ $json.id }}/listings",
        "options": { "authentication": "genericCredentialType", "genericAuthType": "httpHeaderAuth" }
      },
      "notes": "Listings are per-product: there is no cross-product listing enumeration in the API. An empty page is a product with no listing yet."
    },
    {
      "id": "00000000-0000-0000-0000-000000000007",
      "name": "Split out listings",
      "type": "n8n-nodes-base.splitOut",
      "typeVersion": 1,
      "position": [1320, -160],
      "parameters": { "fieldToSplitOut": "items", "options": {} },
      "notes": "Turns the listing page envelope into one item per listing."
    },
    {
      "id": "00000000-0000-0000-0000-000000000008",
      "name": "Loop over listings",
      "type": "n8n-nodes-base.splitInBatches",
      "typeVersion": 3,
      "position": [1540, -160],
      "parameters": { "options": {} },
      "notes": "One listing at a time. Output 0 is the done branch (back to the product loop); output 1 is the loop branch (to the fetch)."
    },
    {
      "id": "00000000-0000-0000-0000-000000000009",
      "name": "Fetch listing source",
      "type": "n8n-nodes-base.httpRequest",
      "typeVersion": 4.2,
      "position": [1760, -320],
      "disabled": true,
      "parameters": { "url": "=PLACEHOLDER-PER-SOURCE", "options": {} },
      "notes": "PLACEHOLDER - disabled until the source is verified per ADR-0012: no endpoint is asserted, each source is verified before integration."
    },
    {
      "id": "00000000-0000-0000-0000-000000000010",
      "name": "Normalize and validate price",
      "type": "n8n-nodes-base.code",
      "typeVersion": 2,
      "position": [1980, -320],
      "parameters": {
        "jsCode": "const item = $input.first().json;\n// PLACEHOLDER - validate the price string, build the idempotency\n// fingerprint from product, listing, observed price and collected-at,\n// and mark a suspicious or missing price as a failure (never as a\n// current price).\nreturn [{ json: {\n  ...item,\n  listing_id: item.id,\n  observation: {\n    amount: 'PLACEHOLDER-four-decimal-string',\n    currency: item.currency,\n    observed_at: 'PLACEHOLDER-collected-instant-rfc3339',\n    fingerprint: 'PLACEHOLDER-derived-identity',\n    availability: 'PLACEHOLDER-available-unavailable-unknown',\n    run_id: $('Start run').first().json.id,\n  },\n} }];"
      },
      "notes": "Why Code: price-string validation and fingerprint construction are item-level parsing no core node expresses; the alert rules themselves stay in ATOM. The run id comes from the Start run node, so the observation is attributed to this execution."
    },
    {
      "id": "00000000-0000-0000-0000-000000000011",
      "name": "Post observation (idempotent)",
      "type": "n8n-nodes-base.httpRequest",
      "typeVersion": 4.2,
      "position": [2200, -320],
      "parameters": {
        "method": "POST",
        "url": "={{ $env.ATOM_API_URL }}/api/v1/listings/{{ $json.listing_id }}/observations",
        "sendBody": true,
        "specifyBody": "json",
        "jsonBody": "={{ JSON.stringify($json.observation) }}",
        "options": {
          "authentication": "genericCredentialType",
          "genericAuthType": "httpHeaderAuth",
          "response": { "response": { "fullResponse": true, "neverError": true } },
          "retry": { "retryOnFail": true, "maxTries": 3, "waitBetweenTries": 2000 }
        }
      },
      "notes": "The idempotent write: 201 new, 200 absorbed - the same body either way. fullResponse keeps the status code for the verification step; neverError routes the 4xx classification to that step instead of failing the node. Retry options per the contract §5."
    },
    {
      "id": "00000000-0000-0000-0000-000000000012",
      "name": "Verify absorption",
      "type": "n8n-nodes-base.code",
      "typeVersion": 2,
      "position": [2420, -320],
      "parameters": {
        "jsCode": "const response = $input.first().json;\nconst sent = $('Normalize and validate price').first().json;\nconst status = response.statusCode;\n\n// A credential refusal is not a data failure: stop the run (contract §5).\nif (status === 401) {\n  throw new Error('credential refused - finish the run failed and rotate the key');\n}\n// Any other 4xx is a data failure of this item (contract §5): count it.\nif (status >= 400) {\n  return [{ json: { ...sent, ok: false, reason: `observation POST answered ${status}` } }];\n}\n\n// The absorption verification (contract §4): the returned row must be\n// the tuple that was sent. A mismatch means the fingerprint is not\n// unique to the intended collection - a fingerprint-construction\n// failure, never a success.\nconst row = response.body;\nconst mismatched =\n  row.amount !== sent.observation.amount ||\n  row.observed_at !== sent.observation.observed_at ||\n  row.listing_id !== sent.listing_id ||\n  row.fingerprint !== sent.observation.fingerprint;\nif (mismatched) {\n  return [{ json: { ...sent, ok: false, reason: 'stored row does not match the sent tuple - fingerprint-construction failure' } }];\n}\nreturn [{ json: { ...sent, ok: true, absorbed: status === 200, observation_id: row.id } }];"
      },
      "notes": "Why Code: comparing the returned row against the tuple that was sent, across the 200/201/4xx cases, is item-level verification no core node expresses (contract §4)."
    },
    {
      "id": "00000000-0000-0000-0000-000000000016",
      "name": "Count run alerts",
      "type": "n8n-nodes-base.httpRequest",
      "typeVersion": 4.2,
      "position": [1100, 40],
      "parameters": {
        "url": "={{ $env.ATOM_API_URL }}/api/v1/alerts",
        "sendQuery": true,
        "queryParameters": { "parameters": [ { "name": "limit", "value": "250" } ] },
        "options": { "authentication": "genericCredentialType", "genericAuthType": "httpHeaderAuth" }
      },
      "credentials": { "httpHeaderAuth": { "id": "PLACEHOLDER-credential-id", "name": "ATOM API key" } },
      "notes": "Aggregates what the rules raised: the finish body's alerts count is the alerts whose triggered_at is at or after the run's started_at. ATOM evaluates the rules; the workflow only reads alerts (contract section 6)."
    },
    {
      "id": "00000000-0000-0000-0000-000000000013",
      "name": "Patch run finish",
      "type": "n8n-nodes-base.httpRequest",
      "typeVersion": 4.2,
      "position": [1100, 160],
      "parameters": {
        "method": "PATCH",
        "url": "={{ $env.ATOM_API_URL }}/api/v1/automation/runs/{{ $('Start run').first().json.id }}",
        "sendBody": true,
        "specifyBody": "json",
        "jsonBody": "={ \"outcome\": \"PLACEHOLDER-ok-partial-failed\", \"observations\": 0, \"alerts\": 0, \"failures\": 0 }",
        "options": { "authentication": "genericCredentialType", "genericAuthType": "httpHeaderAuth" }
      },
      "notes": "PLACEHOLDER outcome and counts - the finish closes the run opened by Start run: outcome ok/partial/failed, the three counts, an error_summary for the failures (failures are data). A second finish is a 409."
    },
    {
      "id": "00000000-0000-0000-0000-000000000014",
      "name": "Any failures?",
      "type": "n8n-nodes-base.if",
      "typeVersion": 2,
      "position": [1320, 160],
      "parameters": {
        "conditions": {
          "options": { "caseSensitive": true, "leftValue": "", "typeValidation": "strict" },
          "conditions": [],
          "combinator": "and"
        }
      },
      "notes": "PLACEHOLDER condition - true when the run summary reports any failed listing."
    },
    {
      "id": "00000000-0000-0000-0000-000000000015",
      "name": "Error path (notify owner)",
      "type": "n8n-nodes-base.noOp",
      "typeVersion": 1,
      "position": [1540, 160],
      "parameters": {},
      "notes": "PLACEHOLDER - the notification channel is a per-deployment decision; it carries no ATOM credential."
    }
  ],
  "connections": {
    "Schedule Trigger": { "main": [ [ { "node": "Start run", "type": "main", "index": 0 } ] ] },
    "Start run": { "main": [ [ { "node": "Get active products", "type": "main", "index": 0 } ] ] },
    "Get active products": { "main": [ [ { "node": "Split out products", "type": "main", "index": 0 } ] ] },
    "Split out products": { "main": [ [ { "node": "Loop over products", "type": "main", "index": 0 } ] ] },
    "Loop over products": { "main": [
      [ { "node": "Count run alerts", "type": "main", "index": 0 } ],
      [ { "node": "Get product listings", "type": "main", "index": 0 } ]
    ] },
    "Count run alerts": { "main": [ [ { "node": "Patch run finish", "type": "main", "index": 0 } ] ] },
    "Get product listings": { "main": [ [ { "node": "Split out listings", "type": "main", "index": 0 } ] ] },
    "Split out listings": { "main": [ [ { "node": "Loop over listings", "type": "main", "index": 0 } ] ] },
    "Loop over listings": { "main": [
      [ { "node": "Loop over products", "type": "main", "index": 0 } ],
      [ { "node": "Fetch listing source", "type": "main", "index": 0 } ]
    ] },
    "Fetch listing source": { "main": [ [ { "node": "Normalize and validate price", "type": "main", "index": 0 } ] ] },
    "Normalize and validate price": { "main": [ [ { "node": "Post observation (idempotent)", "type": "main", "index": 0 } ] ] },
    "Post observation (idempotent)": { "main": [ [ { "node": "Verify absorption", "type": "main", "index": 0 } ] ] },
    "Verify absorption": { "main": [ [ { "node": "Loop over listings", "type": "main", "index": 0 } ] ] },
    "Patch run finish": { "main": [ [ { "node": "Any failures?", "type": "main", "index": 0 } ] ] },
    "Any failures?": { "main": [ [ { "node": "Error path (notify owner)", "type": "main", "index": 0 } ] ] }
  }
}
```

## 3. Placeholder inventory

Everything that must be filled before a first manual run:

1. **Cadence** — Schedule Trigger, after the source verification.
2. **Page size** — the `limit` query parameter of *Get active products*
   and the offset multiplier in its pagination must agree; both are the
   one placeholder.
3. **The fetch node** — per verified source: URL, authentication, rate
   limit, robots/terms notes; then and only then `disabled: false`.
4. **Price parsing** — the *Normalize and validate price* code node's
   validation and fingerprint construction, per source format.
5. **The finish body** — the outcome mapping (`ok` / `partial` / `failed`)
   and the counts: `observations` and `failures` come from the chain,
   `alerts` from *Count run alerts* (`GET /api/v1/alerts`), and
   `error_summary` from the failures.
6. **The IF condition** — against the run summary shape.
7. **The notification channel** — owner decision; no ATOM credential in it.
8. **The credential reference** — the n8n Header Auth credential holding
   the `atom_` key (never the key itself —
   [`../local-setup.md`](../local-setup.md) §3).

The routes themselves are **not** placeholders: every ATOM call above is
the published operation — `POST /api/v1/automation/runs`,
`GET /api/v1/products`, `GET /api/v1/products/{product_id}/listings`,
`POST /api/v1/listings/{listing_id}/observations`,
`PATCH /api/v1/automation/runs/{run_id}` — as documented in
[`../../features/price-monitoring/api.md`](../../features/price-monitoring/api.md)
and pinned by `src/api/routes.rs`.

## 4. Failure handling

Exactly as the contract defines: retryable vs non-retryable classification,
bounded backoff, `401` stops the run, `422` fails the item and is counted —
see [`../integration-contract.md`](../integration-contract.md) §5, §6 and
§7. Duplicate events absorb (§4 of the contract); the workflow never
re-POSTs a `200` absorption — and a `200` counts as success only after the
*Verify absorption* node confirms the stored row is the tuple that was
sent (§4's verification rule).

## 5. Related documents

- [`../integration-contract.md`](../integration-contract.md) — the binding contract.
- [`data-sources.md`](data-sources.md) — the verification gate; all rows unverified.
- [`../local-setup.md`](../local-setup.md) — running it locally, safely.
- [`../../decisions/ADR-0012-automation-orchestration-scope.md`](../../decisions/ADR-0012-automation-orchestration-scope.md) — the accepted boundary and collection rules.
