# Reimbursements — architecture

> **Status:** Accepted
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-REIMB-001`](specification.md)

How the reimbursement feature is built. The public contract is in
[api.md](api.md); the agreed requirements are in
[specification.md](specification.md).

**Two layers exist and two do not.** Migration `0004_expense_claims.sql`
creates `expense_claims`, `expense_items` and the `expense_claim_status` enum;
`src/reimbursements/` holds `mod.rs`, `models.rs`, `dto.rs`, `service.rs`,
`repository.rs` and `handlers.rs`, and five **claim** operations are served under
`/api/v1/claims`. The **item** endpoints and the **lifecycle** actions do not
exist: `POST`, `PATCH` and `DELETE …/items`, and `POST …/submit`,
`…/approve`, `…/reject`, `…/pay`. Everything below that names an item write or a
transition is the **plan** for R3–R5, recorded so that the implementing change
has a shape to follow. A planned design is recorded as planned; the text is
corrected whenever the implementation and this plan disagree.

## 1. Responsibilities

| Module | Responsibility | Exists |
| ------ | -------------- | ------ |
| [`../../../src/reimbursements/handlers.rs`](../../../src/reimbursements/handlers.rs) | HTTP only: extraction, status codes, response serialization. Five handlers with their `utoipa` attributes. | Yes |
| [`../../../src/reimbursements/service.rs`](../../../src/reimbursements/service.rs) | Business rules: validation, the project rule, the mapping from a repository miss to `404`. The lifecycle rules arrive with R4. | Yes, claim rules only |
| [`../../../src/reimbursements/repository.rs`](../../../src/reimbursements/repository.rs) | SQL: the claim statements, every one scoped by `user_id`. The item statements and the total recomputation arrive with R3. | Yes, claim statements only |
| [`../../../src/reimbursements/models.rs`](../../../src/reimbursements/models.rs) | Persistence shapes: `ExpenseClaim`, `ExpenseItem`, `ClaimStatus`. | Yes |
| [`../../../src/reimbursements/dto.rs`](../../../src/reimbursements/dto.rs) | Public contracts: `ClaimResponse`, `ClaimDetailResponse`, `ClaimPage`, `ItemResponse`, `CreateClaimRequest`, `UpdateClaimRequest`, `ClaimQuery`, `ClaimsListQuery` and the two length limits. **No money type** — see §2.3. | Yes |

`mod.rs` declares all five modules and the module is registered in `src/lib.rs`.
`ClaimStatus` is one enum used in three places — a filter, a response and the
database — with `snake_case` naming and `sqlx(type_name = "expense_claim_status")`,
so no second copy exists in `dto.rs`.

The layer rule is `AGENTS.md` §2: a handler extracts and picks a status, calls
one service method, and never reaches the database; the repository is the only
file that holds SQL.

The feature depends on:

| Dependency | For |
| ---------- | --- |
| `src/api/middleware.rs` (`AuthUser`) | The caller's identity, and therefore the ownership predicate. |
| `src/pagination.rs` | `PaginationQuery`, `Page<T>`, the list envelope and the clamp. |
| `src/helpers/validation.rs` | The shared text validators (trimmed, bounded, required, search). |
| `src/error.rs` (`AppError`) | The one error envelope. |
| `src/config/messages.toml` | Every client-facing string, through the message catalogue. |

It reads `projects` only to verify the optional link, and lets the foreign key do
the clearing. It does not read `notes` or `tasks` at all.

## 2. Interfaces

### 2.1 `ClaimService`

The five claim methods exist; the item and lifecycle methods are the plan for
R3 and R4.

| Method | Contract | Exists |
| ------ | -------- | ------ |
| `create(owner, request)` | Validates title, description and currency; verifies the optional project belongs to `owner`; inserts the claim in `draft`; returns `ClaimDetailResponse`. | Yes |
| `list(owner, query, pagination)` | Applies the filters and the clamp; returns `Page<ClaimResponse>` without items. | Yes |
| `get(owner, claim_id)` | Loads the claim **and its items**, both owned by `owner`; `404` otherwise. | Yes |
| `update(owner, claim_id, request)` | Validates only the fields present; applies a partial update; `404` otherwise, naming the project when the project is what failed. | Yes |
| `delete(owner, claim_id)` | Deletes the claim, its items cascade; `404` otherwise. | Yes |
| `add_item(owner, claim_id, request)` | Requires `draft`; inserts the item and recomputes the total in one transaction. | No — R3 |
| `update_item(owner, claim_id, item_id, request)` | Requires `draft`; updates the present fields and recomputes the total in one transaction. | No — R3 |
| `delete_item(owner, claim_id, item_id)` | Requires `draft`; deletes the item and recomputes the total in one transaction. | No — R3 |
| `submit(owner, claim_id)` | `draft` → `submitted` after the preconditions; `rejected` → `draft` (reopen only); anything else `409`. | No — R4 |
| `decide(owner, claim_id, decision)` | `submitted` → `approved` or `rejected`; anything else `409`. | No — R4 |
| `pay(owner, claim_id)` | `approved` → `paid`; anything else `409`. | No — R4 |

The `owner` argument is the authenticated `user_id`. It is the first parameter
of every method, which makes it impossible to call a claim method without
saying whose claim is meant. `AppState` builds the service once, from the pool,
the message catalogue and the configured pagination, and the field is
`state.reimbursements`.

`list` returns `Page<ClaimResponse>`, not `Page<ClaimDetailResponse>`: the
listing does not carry items.

### 2.2 `ClaimRepository`

Six methods, no trait, and **nine static statements** behind them: five
operations are one statement each, and the listing is a page statement plus a
matching `COUNT(*)` in each of its two shapes. At most two statements run per
request.

| Method | What the statement does | Ownership predicate |
| ------ | ----------------------- | ------------------- |
| `insert` | `INSERT … SELECT … WHERE <project is null> OR EXISTS (SELECT 1 FROM projects WHERE id = $n AND user_id = $owner) RETURNING …`. Writes `'draft'` explicitly; `total_amount` comes from the column default and renders `"0.0000"`. | The `EXISTS` names the caller. `user_id` is written from the token, never from the request. |
| `find_owned` | The single-row read of a claim. | `id = $1 AND user_id = $2` |
| `list_items_owned` | Reads the items of one claim for the detail response. **Read-only in R2**: it is not an item endpoint. | `claim_id = $1 AND user_id = $2` — both, always |
| `update_owned` | The partial update: `COALESCE($n, column)` per field, an explicit `CASE` for the one field that can be set to `NULL`, `updated_at = NOW()`, and the project rule in the same `WHERE`. | `id = $1 AND user_id = $2`, plus `user_id = $2` inside the `EXISTS` over `projects` |
| `delete_owned` | One `DELETE`; the items follow through `ON DELETE CASCADE`. | `id = $1 AND user_id = $2` |
| `list_owned` | The page and its count, in two shapes — §2.5. | `user_id = $1` on both |

Four rules the statements follow:

1. **`user_id` is in the `WHERE` clause, not in a check.** This is
   [`ADR-0005`](../../decisions/ADR-0005-ownership-enforced-in-sql.md). A miss
   becomes `AppError::NotFound` in the service, with the same response for
   "absent" and "another account's".
2. **The project rule is in the write statement, not in a preceding read.** The
   insert and the update either produce a row or none, so a project deleted
   between a check and a write is not a possible state, and the success path
   costs no extra round trip. `None` on the update path is ambiguous — the
   claim or the project — and the service resolves it with **one** owner-scoped
   `find_owned` *after* the statement has already refused; see
   [`../../architecture/reimbursements-crud-design.md`](../../architecture/reimbursements-crud-design.md)
   §6.4.
3. **The item predicate carries both `claim_id` and `user_id`**, so an item
   cannot be reached by combining an id from one account with a claim id from
   another.
4. **Money is selected with `::text` and an explicit alias.** No `numeric`
   reaches Rust as anything but its exact rendering
   ([`ADR-0007`](../../decisions/ADR-0007-money-representation.md)). A money
   column selected without `::text` does not decode at all, so the mistake fails
   loudly instead of publishing a number.

`update_owned` needs its second statement only on the failure path. That is a
deliberate trade, recorded as a limitation in §10.

### 2.3 Money: a string, not a type

**There is no `Money` newtype, and R2 did not add one.** The design that settled
this is
[`../../architecture/reimbursements-crud-design.md`](../../architecture/reimbursements-crud-design.md)
§4.3, and the reasoning is worth keeping because a previous version of **this
document** promised the newtype:

1. A newtype would be a **second wire contract**. The field's JSON type is
   already fixed by `String` plus a validator, and a `Deserialize` impl that
   refuses a number produces exactly the same response as `String` does: the
   shared extractor reports a body type mismatch as one violation for the field
   `body` ([`../../api/error-contract.md`](../../api/error-contract.md) §3), and
   a newtype cannot name the field any better, because the extractor only ever
   sees the whole-body error.
2. It would have no method but a constructor. Every operation money needs — the
   sum, the range check, the comparison — is SQL's.
3. R2 accepts **no** money at all, so the type would have zero callers until R3,
   and the module-admission rule in [`../../../src/helpers/mod.rs`](../../../src/helpers/mod.rs)
   asks for more than one caller.

What exists instead:

- Every money field is `String` in the DTO and comes from a `::text` select.
  `NUMERIC(19,4)::text` renders exactly four decimals (`"149.8000"`,
  `"0.0000"`), which **is** the wire form; serde quotes it as a JSON string.
- No `f32`, no `f64`, no decimal crate, and no arithmetic in Rust. The total is
  computed by `SUM` in SQL, and not at all in R2.
- The four-decimal **input** rule is fixed now and built in R3, as one function
  beside the other text validators (`validate_money` in
  `src/helpers/validation.rs`), not as a type. It is one character-level check
  that produces a `FieldViolation` naming the field, which is what that module
  already holds.

The database column is permissive on **input** and exact on **output**: it
coerces `"12.5"` to `12.5000` and rounds a fifth decimal instead of refusing it.
That is why the input rule cannot be left to the column, and why
`money_is_stored_with_four_decimal_places` in
[`../../../tests/claims_schema.rs`](../../../tests/claims_schema.rs) records the
behaviour rather than treating the scale as validation.

### 2.4 `project_id` is a tri-state

`project_id` is the one field of a claim whose "absent" and "null" mean different
things, so `UpdateClaimRequest.project_id` is `Option<Option<Uuid>>` — and that
shape alone is **not enough**, because `serde` collapses an explicit `null` into
"absent" for a plain nested `Option`:

```rust
#[serde(default, deserialize_with = "double_option")]
pub project_id: Option<Option<Uuid>>,
```

`#[serde(default)]` handles absence (`None`); `double_option` — eight lines of
`serde`, not a `serde_with` dependency — wraps a present value, so an explicit
`null` becomes `Some(None)`. Dropping either half makes "clear the link"
inexpressible.

| Body | Deserialised | Effect |
| ---- | ------------ | ------ |
| key absent | `None` | untouched — `NULL` to the `COALESCE` |
| `"project_id": null` | `Some(None)` | cleared — the `CASE WHEN` writes `NULL` |
| `"project_id": "<uuid>"` | `Some(Some(id))` | set, after the ownership predicate |

The mapping to the two bound parameters lives in one accessor,
`UpdateClaimRequest::project_change() -> (clear, Option<Uuid>)`, so no handler
can reconstruct it differently. The unit test
`a_project_link_is_kept_cleared_or_set_by_the_three_body_forms` deserialises all
three bodies and asserts both halves of the pair, so neither the attributes nor
the accessor can drift;
`the_update_claim_request_publishes_a_nullable_project_id` asserts the same
seam is visible in the OpenAPI document as a nullable field rather than a
missing one.

`title` and `description` have no tri-state: `{"title": null}` is indistinguishable
from an omitted key and is treated as omitted, `{"title": ""}` is a `422`, and
`{"description": ""}` is the one empty string that means something — it clears
the description, which is the column's own default.

### 2.5 The list: two shapes, one per index

`schema.md` §11.1 gives two listing indexes and says why. A single statement
written as `($n IS NULL OR status = $n)` would hide the status from the planner
and leave `expense_claims_user_id_status_created_at_idx` unread, so the
repository holds two statement shapes and picks one:

| Request | Shape | Index |
| ------- | ----- | ----- |
| no `status` | `user_id = $1`, then currency, the date bounds and the search as `($n IS NULL OR …)` | `expense_claims_user_id_created_at_idx (user_id, created_at DESC)` |
| with `status` | the same `SELECT`, `FROM` and `WHERE user_id = $1`, plus `status = $2::expense_claim_status`, and the remaining parameters shifted by one | `expense_claims_user_id_status_created_at_idx (user_id, status, created_at DESC)` |

Each shape has a matching `COUNT(*)` with the same `WHERE` and no
`ORDER`/`LIMIT`, so `pagination.total` is the size of the set the page was taken
from, never the whole collection. The order is `created_at DESC, id DESC` in
both, so it is total and two pages cannot overlap or skip a row.

Four details the statements carry:

1. **Every optional parameter has an explicit cast** (`::char(3)`, `::date`,
   `::text`, `::expense_claim_status`). A parameter that appears only as
   `$n IS NULL` gives PostgreSQL nothing from which to infer its type.
2. **The date bounds are UTC days, half-open at the top**: `>= from 00:00Z` and
   `< (to + 1) day 00:00Z`, which is inclusive of both ends without a
   `23:59:59.999999` and without depending on the session `TimeZone`.
3. **A draft drops out when either bound is present**, by SQL's own semantics:
   a comparison against a `NULL` `submitted_at` is `NULL`, and `NULL` is not
   true.
4. **The search term is unescaped**, exactly as `notes` and `projects` use it: a
   `%` or `_` is a wildcard. That is existing API behaviour, and changing it in
   one feature would make two searches behave differently.

`currency`, the date range and the search are not indexed and must not be: they
narrow the rows the chosen index has already reduced to the owner.
`expense_claims_project_id_idx` is not read by R2 — the published listing has no
`project_id` filter.

### 2.6 The lifecycle guard — planned for R4

One function, `ensure_transition(current, action) -> Result<Next, AppError>`,
holds the transition table, and the service calls it before every state
change. A table with one implementation is what makes "every forbidden
transition is refused" a single testable rule rather than eight `if`
statements. It does not exist yet, and neither does any writer of `status`.

## 3. Data ownership

| Table | Owner column | Meaning |
| ----- | ------------ | ------- |
| `expense_claims` | `user_id` → `users (id) ON DELETE CASCADE` | A claim belongs to exactly one account. |
| `expense_items` | `user_id` → `users (id) ON DELETE CASCADE`, and `(claim_id, user_id)` → `expense_claims (id, user_id)` | An item belongs to one claim and to one account, and the two cannot disagree. |

`expense_items.user_id` is redundant with the claim's owner, and it is kept on
purpose: it lets the item statements carry the ownership predicate directly,
without a join that a future query could forget. The **composite foreign
key** makes a row whose owner differs from its claim's owner unrepresentable,
which is stronger than a service check. The parent therefore needs a unique
index on `(id, user_id)`; the migration creates it as
`expense_claims_id_user_id_unique_idx`, and
[`schema.md`](../../database/schema.md) records it.

The optional link is `expense_claims.project_id → projects (id) ON DELETE SET
NULL`. Deleting a project clears the reference and leaves the claim.

The physical definition — column types, the `CHECK` constraints, the indexes —
belongs in [`schema.md`](../../database/schema.md) §11, which is the
authority the migration implements. The plan's working sketch is in
[`plan.md`](../../engineering/runs/chain/plan.md) §5.1.

### 3.1 The seeded claims

`atom-seed` seeds claims and items in the same chain that adds the tables,
because its count check compares its own table list against
`information_schema` and every run would otherwise fail. The profiles are
**1 / 25 / 50 claims** and **0 / 48 / 98 items** for `minimal` / `dev` /
`demo`. Claim `i` gets `1 + (i % 3)` items **except claim `0`**, the
profile's first claim, which is a `draft` (`status` cycles the five states by
`i % 5`) and gets none — that single exception is why `minimal` seeds no items
at all.
`currency` cycles a fixed three, and `submitted_at`, `decided_at` and `paid_at`
are set so that `expense_claims_lifecycle_timestamps_chk` holds for every row.
`total_amount` is the `SUM` of the claim's items, computed in SQL. The seeder
asserts the counts exactly and re-reads every seeded claim's total against its
items; `tests/claims_schema.rs` owns the constraint proof, the seeder owns the
data.

The API can create neither a status other than `draft` nor a total other than
`0.0000`, so the endpoint tests insert their own claims and items with SQL for
the filters, the money scale and the item order — the fixture section of
[testing.md](testing.md) §2. A fixture is never a seeded row: every one is
scoped to the account the test registered, and none of the seeder's data is
needed to run the API suite.

## 4. Invariants

| # | Invariant | Enforced by | Exists |
| - | --------- | ----------- | ------ |
| 1 | A claim belongs to exactly one user; an item belongs to one claim and to the same user. | The non-null foreign keys, and `user_id` in the item predicate. | Yes |
| 2 | A claim or item of another account is invisible and answers `404`. | `user_id = $n` in every statement; `AppError::NotFound` in the service. | Claim half yes; item half R3 |
| 3 | `total_amount = SUM(items.amount)`, always. | Recomputed by `SUM` in SQL in the same transaction as every item write. | **No** — R3 writes the items; the seeder and the schema tests hold the data half |
| 4 | One currency per claim, never converted. `currency` is required and upper-cased on create and is **not patchable**: a `PATCH` body carrying it — or `status`, or `total_amount` — is **accepted with `200` and the field is ignored**, because none of the three is a field of `UpdateClaimRequest` and none appears in any `SET` clause. There is no `422` for them; that is the published `api.md` §6 behaviour, and [status.md](status.md) §1.1 records why the earlier "service rule with a `422`" wording was wrong. | The create validator, the three-field `UpdateClaimRequest`, and the `SET` clause of `update_owned`. Asserted by `the_claim_currency_is_not_patchable` and `the_claim_status_is_not_patchable`. | Yes |
| 5 | An amount is non-negative and at most 15 integer digits. | The R3 money validator, and a database `CHECK (amount >= 0)` plus the column type. | Schema half yes; the validator is R3 |
| 6 | A claim in any status other than `draft` has at least one item and a positive total. | The submit precondition; a `CHECK` cannot see the items, so this one is a service rule. | **No** — R4 |
| 7 | Only the listed transitions happen; `paid` is terminal. | `ensure_transition`, and a `status` `CHECK` for the five values. | **No** — R4 |
| 8 | A partial update leaves an omitted field untouched; `project_id: null` clears the link; an empty body changes only `updated_at`. | `Option`/`Option<Option<…>>` in the DTO (§2.4) and `COALESCE` plus an explicit `null` path in SQL. | Yes |
| 9 | The list order is total and stable. | `ORDER BY created_at DESC, id DESC`. | Yes |
| 10 | The item order is chronological and total. | `ORDER BY incurred_on, created_at, id`. | The read exists; the order is asserted by `the_claim_detail_items_are_in_chronological_order` |
| 11 | A receipt is a reference, never bytes. | The column is `text`; no route accepts a file ([`ADR-0009`](../../decisions/ADR-0009-receipt-references.md)). | Yes, vacuously: no route accepts a receipt at all yet |
| 12 | Deleting a project keeps the claim. | `ON DELETE SET NULL`. | Yes |

The migration enforces the **data** half of invariants 1, 5, 11 and 12: the two
`user_id` keys and the composite item key, `expense_items_amount_chk`, the
`receipt_url` column type, and `ON DELETE SET NULL`. Invariants 2, 4, 8 and 9 are
now held by the claim statements as well as by the migration. Invariants 3, 5
(input), 6 and 7 remain service rules that arrive with R3 and R4. A constraint
holds data; it does not answer a request.

## 5. Error handling

| Situation | Error | Status | Exists |
| --------- | ----- | ------ | ------ |
| Title, description or currency invalid, on create | `validation_failed` naming the field | `422` | Yes |
| `search` over its bound, `currency` filter not three letters, reversed date range | `validation_failed` naming the field | `422` | Yes |
| Item description, amount, date, category or receipt invalid | `validation_failed` naming the field | `422` | **No** — R3 |
| Submission with no items or a zero total | `validation_failed` naming `items` or `total_amount` | `422` | **No** — R4 |
| The claim does not exist, or belongs to another account | `not_found` | `404` | Yes |
| The optional project does not exist, or belongs to another account | `not_found` | `404` | Yes |
| The item does not exist, or does not belong to that claim | `not_found` | `404` | **No** — R3 |
| A transition the lifecycle does not allow | `conflict` | `409` | **No** — R4 |
| An item write on a claim that is not a draft | `conflict` | `409` | **No** — R3 |
| The body is not JSON, or a path parameter is not a UUID | `bad_request` | `400` | Yes |
| A query parameter cannot be parsed, or is outside its enum | `bad_request` | `400` | Yes |
| A body value of the wrong type, or a missing required field | `validation_failed`, field `body` | `422` | Yes |
| No or invalid credentials | `unauthorized` | `401` | Yes |
| A body over the limit | `payload_too_large` | `413` | Yes |
| The database failed | `database_error` | `500` | Yes |

**Nothing in this feature answers `409` today**, because nothing changes a
status. The two `409` rows are R3 and R4.

Two rules the claim service follows, in this order:

1. **Validate the whole body first, then touch the database.** A request that is
   both malformed and aimed at a foreign resource is a `422`, not a `404`.
2. **Every repository miss becomes `NotFound` with a catalogue message.** The
   service never compares identifiers and never returns `403` for a resource
   ([`ADR-0005`](../../decisions/ADR-0005-ownership-enforced-in-sql.md) §3).

A missing claim and a claim of another account produce the **same** response.
That is deliberate: a `403` would confirm that the identifier exists.

## 6. The security boundary

```text
                 ┌──────────────────────────────────────────────┐
  bearer token ─►│ /claims, /claims/{claim_id}                  │  served (R2)
                 │ /claims/{claim_id}/items                     │  planned (R3)
                 │ /claims/{claim_id}/{submit|approve|reject|pay}│  planned (R4)
                 └──────────────────────────────────────────────┘
                                    │
                                    │  user_id from the credential only
                                    ▼
                 every statement carries `user_id = $n`
                 the item statements carry `claim_id` **and** `user_id`
```

- There is no anonymous route in this feature.
- A claim's `project_id` is verified against `projects.user_id` **inside the
  write statement**, so a claim cannot point at another account's project and
  the check cannot be skipped by a caller who sends the field late.
- An item id is only meaningful **inside** its claim. The path
  `/claims/{claim_id}/items/{item_id}` is resolved as a pair, so a caller
  cannot move an item between claims by guessing an id. Planned for R3.
- The receipt URL is owner data. It is never fetched (no SSRF) and never
  logged (it can carry a signed credential). Planned for R3.
- `user_id` is never accepted from a request body and never appears in a
  response: the DTOs drop it, and the statements write it from the token.

## 7. The request path

### 7.1 Updating a claim — served

```text
PATCH /api/v1/claims/{claim_id}
  │
  ├─ api::middleware::AuthUser → user_id
  ├─ api::extract::Path                                 not a UUID → 400
  ├─ api::extract::Json<UpdateClaimRequest>             not JSON → 400
  │                                                     wrong type → 422, field `body`
  ├─ reimbursements::service::ClaimService::update
  │    ├─ validate title / description, when present    → 422 naming the field
  │    ├─ project_change()                              (clear, Option<Uuid>)
  │    ├─ UPDATE … WHERE id = $1 AND user_id = $2 (+ the project EXISTS)
  │    │      0 rows → find_owned($1, $2)
  │    │                ├─ a row  → the claim exists, so the project failed
  │    │                │            → 404 not_found, project message
  │    │                └─ no row → 404 not_found, claim message
  │    └─ 200 OK, ClaimDetailResponse (the claim, then its items)
```

An ignored field never reaches the service: `currency`, `status` and
`total_amount` are not fields of `UpdateClaimRequest` (§4 invariant 4). A body
with no field at all is still a `200`, and the only change is `updated_at`.

### 7.2 Adding an item — planned for R3

```text
POST /api/v1/claims/{claim_id}/items
  │
  ├─ api::middleware::AuthUser → user_id
  ├─ api::extract::Path<(Uuid,)>                       not a UUID → 400
  ├─ api::extract::Json<CreateItemRequest>             not JSON → 400
  ├─ reimbursements::service::ClaimService::add_item
  │    ├─ validate description, amount, incurred_on, category, receipt_url
  │    ├─ BEGIN
  │    │    ├─ SELECT status FROM expense_claims
  │    │    │    WHERE id = $1 AND user_id = $2        no row → 404
  │    │    ├─ status != 'draft'                       → 409
  │    │    ├─ INSERT INTO expense_items …
  │    │    └─ UPDATE expense_claims SET total_amount = SUM(…), updated_at = NOW()
  │    └─ COMMIT
  └─ 201 Created, ItemResponse
```

The transaction is what makes invariant 3 hold. A failure after the insert
leaves neither the item nor a new total. The claim's new total is read with
`GET /claims/{claim_id}`, which is what the money test does.

### 7.3 Transitioning — planned for R4

```text
POST /api/v1/claims/{claim_id}/approve
  │
  ├─ AuthUser → user_id
  ├─ Path → claim_id
  ├─ ensure_transition(current_status, Approve)
  │    ├─ submitted → approved     set decided_at = NOW()
  │    └─ anything else            → 409
  └─ 200 OK, ClaimDetailResponse
```

## 8. Related files

| File | Role | Exists |
| ---- | ---- | ------ |
| [`../../../src/api/routes.rs`](../../../src/api/routes.rs) | Registers the five claim routes, the five `ROUTE_TABLE` rows and the seven component schemas in the OpenAPI document. | Yes, extended |
| [`../../../src/api/extract.rs`](../../../src/api/extract.rs) | The `Path`, `Query` and `Json` extractors that keep the error envelope intact. | Yes |
| [`../../../src/api/middleware.rs`](../../../src/api/middleware.rs) | `AuthUser`, the source of the ownership predicate. | Yes |
| [`../../../src/pagination.rs`](../../../src/pagination.rs) | The list envelope and the clamp. | Yes |
| [`../../../src/helpers/validation.rs`](../../../src/helpers/validation.rs) | The shared text validators; `validate_money` arrives with R3. | Yes |
| [`../../../src/reimbursements/`](../../../src/reimbursements/) | The feature itself. | All five modules |
| [`../../../migrations/0004_expense_claims.sql`](../../../migrations/0004_expense_claims.sql) | The two tables, their constraints and their indexes. | Yes |
| [`../../../tests/claims_schema.rs`](../../../tests/claims_schema.rs) | The constraint, cascade and money-scale proofs. | Yes |
| [`../../../tests/claims_api.rs`](../../../tests/claims_api.rs) | The 34 claim endpoint tests. | Yes |

## 9. Related decisions

- [`ADR-0007`](../../decisions/ADR-0007-money-representation.md) — money as
  `NUMERIC(19,4)` and a JSON string.
- [`ADR-0009`](../../decisions/ADR-0009-receipt-references.md) — a receipt is a
  reference, not an upload.
- [`ADR-0005`](../../decisions/ADR-0005-ownership-enforced-in-sql.md) —
  ownership as a SQL predicate.
- [`ADR-0001`](../../decisions/ADR-0001-modular-monolith.md) — why this is a
  module and not a service.

No new ADR was written for R2. The five claim endpoints fit the accepted model,
and the one structural decision — no money type, the check is a validator in
`helpers` — narrows a plan in
[`module-boundaries.md`](../../architecture/module-boundaries.md) rather than
reversing a decision. A change that needs money handled in Rust needs an ADR
superseding `ADR-0007`.

## 10. Limitations of this design

- **The total is a stored column, not a view.** It is recomputed on every item
  write inside the transaction, so a direct SQL edit of an item without the
  recomputation leaves the total stale. The seeder's post-insert verification
  checks it on seeded data; the API-level check is R3's, because R2 writes no
  item.
- **The submit precondition is not a database constraint.** A `CHECK` cannot
  count the items of another table, so the "at least one item" rule is
  enforced by the service. A direct insert can violate it.
- **The item `user_id` is redundant.** It is an intentional denormalisation,
  kept so that the item statements carry the ownership predicate without a
  join. The composite foreign key on `(claim_id, user_id)` makes a
  disagreement unrepresentable; the cost is the extra unique index on the
  parent.
- **One transaction per item write, one claim row lock.** Two concurrent item
  writes on the same claim serialise on the claim row. That is correct and it
  is also the throughput ceiling of the feature; a personal workspace will
  never reach it.
- **The lifecycle is a service rule, not a state machine in the database.** A
  `status` change written directly by SQL bypasses it.
- **A `PATCH` with a foreign `project_id` costs one extra owner-scoped read on
  the failure path**, to name the resource correctly. The success path is one
  statement.
- **The detail read is two statements in two snapshots.** Nothing writes items
  in R2, so nothing can race it. `R3` introduces the first item writer and owns
  the decision whether the read needs one snapshot.
- **`?currency=` is a `422`, while `?search=` means "no filter".** Each follows
  the published sentence for that parameter; the difference is in the contract,
  not in the code.
- **A `%` or `_` in `search` is a wildcard.** Identical to `notes` and
  `projects`; fixing it in one feature would make two searches behave
  differently.
- **Deleting a claim is permanent.** There is no soft delete and no history,
  in any status (`REIMB-R11`).
