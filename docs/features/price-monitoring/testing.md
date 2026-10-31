# Price monitoring — testing

> **Status:** `Verified` (reviewed 2026-10-31; remediation re-gated the same day)
> **Last updated:** 2026-10-31

Two integration suites and the whole-surface sweeps cover the feature. The
suite needs the disposable test database:

```bash
TEST_DATABASE_URL=postgres://atom:atom@127.0.0.1:5432/atom_test \
  cargo test --locked --test monitoring_api --test monitoring_schema --test openapi_contract
```

## 1. `tests/monitoring_api.rs` — 34 cases

HTTP-level, driven through `api::router`, one account per test, counts
restricted to the ids each test created. The groups and what they pin:

| Group | Cases | Evidence |
| ----- | ----- | -------- |
| Product CRUD and the derived identity | 6 | `201` with the derived `normalized_name` and 64-hex `config_hash`; a duplicate identity is `409` while a new configuration is not; a patch merges and re-derives (the omitted field untouched, `""` clears `specs`); the four field validations; `DELETE` removes the listings, observations, rules and alerts. |
| Listing CRUD | 5 | Defaults (`unknown`, `valid`, `last_observed_at` null), the upper-cased currency, the duplicate URL `409` per owner (the same URL under another owner is fine), the `external_id` tri-state plus the two reports, addressing only inside the parent product, URL and currency validation. |
| Observations | 6 | The newest-first paginated history; **the absorbed fingerprint** (`201` then `200` with the same id, one row); the four-decimal money rule, the JSON-number refusal, the fingerprint bound; the `latest` read and its `404`s; the foreign listing's `404` on all three verbs; the `run_id` recorded and a foreign run's `404`. |
| Rules | 4 | Born `enabled` on `inbox`, the second rule `409`, the patch toggles the switch, the target and the percent-drop tri-state, the percent-drop and money validations, addressing only inside the parent product and the owner. |
| Trigger and alerts | 7 | The target reached raises exactly one alert; a **new** observation below the target raises a second (new observations may fire again); the retried write of the triggering observation is absorbed and raises **no duplicate**; **the boundary equality — a price exactly at the target — fires** (the SQL predicate is `>=`; a `>` regression would pass every other trigger case); a 4.5% drop does not fire a 10% rule while a 15% drop does; a currency mismatch never fires; a disabled rule never fires; the unread filter and the read mark follow the patch; a foreign alert is a `404` on read and on the mark. |
| Concurrent ingestion | 2 | The two concurrency guarantees of the observation transaction, raced with real overlapping requests (`tokio::join!`, the `tests/auth_api.rs` refresh-race mechanism, no sleeps) and **verified in SQL afterwards**: two distinct fingerprints concurrently → both `201`, two rows, two alerts (the serialised evaluations each see the other's committed row); the same fingerprint concurrently → exactly one `201` and one `200` returning the winner's row, exactly one row, exactly one alert. The asserted end state holds for any interleaving — the listing's `FOR UPDATE` lock decides who wins, not the test's timing. |
| Runs | 3 | Start `running` → finish once with counts and a trimmed summary → second finish `409`; the finish validations (`running` refused, negative counts, empty summary, and an omitted tri-state leaves the summary); a foreign run's `404` with the run untouched. |
| Surface | 2 | The product list pagination and the status/search filters; **all 22 operations answer `401` to an unauthenticated call**, before the body is read. |

## 2. `tests/monitoring_schema.rs` — 14 cases

SQL against the real database, in the scratch-database pattern of
`tests/claims_schema.rs`; every rejection asserts the SQLSTATE **and** the
constraint name:

| Case | Pins |
| ---- | ---- |
| The migration applies from empty | A scratch database with zero tables ends up with migrations `1..=8` and all six tables. |
| The idempotency index | The same `(user, listing, fingerprint)` twice is `23505` naming `price_observations_user_id_listing_fingerprint_unique_idx`; the same fingerprint under another listing is a different observation. |
| The duplicate-alert index | The same `(rule, observation)` twice is `23505` naming `price_alerts_rule_observation_unique`. |
| The controlled vocabularies | The accepted values of all six sets round-trip; the refused ones are `23514` naming their `CHECK`s, including the upper-case config digest and the `running`-without-a-finish lifecycle. |
| Money | `NUMERIC(19,4)` stores at scale four (`::text` is the wire form); a negative amount and a negative target are `23514`; a five-decimal input is rounded by the column — which is why the service refuses it before the statement runs. |
| Composite foreign keys | A cross-owner listing, observation, run reference and alert (both directions) are `23503` naming the composite key; the honest row with the parent's owner is accepted. |
| Cascades | Deleting the product cascades to listings, observations, rules and alerts; deleting the account cascades to all six tables while the bystander's rows survive; a run with observations is refused by `NO ACTION` until the observation is gone. |
| URL and one-rule | The URL `CHECK` refuses a scheme and an over-long URL and accepts one at the bound; a second rule for one product is `23505` while another product's rule is fine. |

## 3. The sweeps and the contract

- `tests/openapi_contract.rs` — the route table, the router and the generated
  document agree in both directions; the surface is pinned at **76
  operations**; the list endpoints publish exactly their parameter sets
  (including `/products`, `/alerts` and `/automation/runs`).
- `tests/contract_sweep.rs` — every published operation answers `401`
  unauthenticated and `400`/`200` as its document says; the ownership sweep
  drives every id-taking operation with a second account's token, including
  the five new placeholders (`product_id`, `listing_id`, `rule_id`, `run_id`,
  `alert_id`), and leaves account A's rows byte-identical.

## 4. Unit tests

In-module: `dto.rs` (the tri-states and the wire forms of the vocabularies),
`service.rs` (the currency, URL, percent-drop, count and summary validators,
the identity determinism and its Unicode bound, the terminal outcome set),
`helpers::validation` (unchanged, reused for money and search).

## 5. What was proven by the review, and what is still not covered

The feature is **`Verified`** as of `ATOM-REVIEW-20261031-001` (2026-10-31):
an independent read-only review returned **VERIFIED WITH CONDITIONS** — its
three conditions were the workflow skeleton's paths (M-1), the missing
concurrent-ingestion proof (M-2) and the client-side absorption guidance
(M-3) — and all three were remediated the same day. The remediation re-ran
every gate:

| Gate | Result |
| ---- | ------ |
| `cargo fmt --all -- --check` | exit `0` |
| `cargo clippy --all-targets --all-features -- -D warnings` | exit `0` |
| `TEST_DATABASE_URL=… cargo test --all-targets --locked --no-fail-fast` | **630 passed, 0 failed** across 29 binaries |
| `cargo run --bin atom-docs -- --strict` | OK, 0 warnings |
| the two concurrent-ingestion cases, run 3× | 3 consecutive green runs (2 passed each) |

The independent review's own pre-remediation run of the whole suite is
recorded in its review entry: 627 passed, 0 failed.

Not covered by any test in this change: an end-to-end workflow run (no n8n
instance exists in the test environment), the delivery of a notification, and
the seeder's monitoring rows beyond the seeder suite's own idempotency and
V6 coverage.
