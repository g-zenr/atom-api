# Reimbursements — architecture

> **Status:** Accepted
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-REIMB-001`](specification.md)

How the reimbursement feature is intended to be built. The public contract is
in [api.md](api.md); the agreed requirements are in
[specification.md](specification.md).

**Nothing in this document exists yet.** No file under
`src/reimbursements/`, no migration and no test is in the repository. The
module names, table names and SQL below are the **plan**, recorded so that the
implementing change has a shape to follow. The design is **accepted but not
built**, and the text is corrected whenever the implementation and this plan
disagree.

## 1. Responsibilities

| Module | Responsibility |
| ------ | -------------- |
| `src/reimbursements/handlers.rs` | HTTP only: extraction, status codes, response serialization. |
| `src/reimbursements/service.rs` | Business rules: validation, the lifecycle, the transaction coordination, the mapping from a repository miss to `404`. |
| `src/reimbursements/repository.rs` | SQL: claim and item statements, every one scoped by `user_id`, and the total recomputation. |
| `src/reimbursements/models.rs` | Persistence shapes: `ExpenseClaim`, `ExpenseItem`. |
| `src/reimbursements/dto.rs` | Public contracts: `ClaimResponse`, `ClaimDetailResponse`, `ClaimPage`, `ItemResponse`, the create and update requests, the query type, the money type and the length limits. |

The feature depends on:

| Dependency | For |
| ---------- | --- |
| `src/api/middleware.rs` (`AuthUser`) | The caller's identity, and therefore the ownership predicate. |
| `src/pagination.rs` | `PaginationQuery`, `Page<T>`, the list envelope and the clamp. |
| `src/helpers/validation.rs` | The shared text validators (trimmed, bounded, required). |
| `src/error.rs` (`AppError`) | The one error envelope. |
| `src/config/messages.toml` | Every client-facing string, through the message catalogue. |

It does not read `projects` except to verify the optional link and to let the
foreign key do the clearing. It does not read `notes` or `tasks` at all.

## 2. Interfaces

### 2.1 `ClaimService`

| Method | Contract |
| ------ | -------- |
| `create(owner, request)` | Validates title, description and currency; verifies the optional project belongs to `owner`; inserts the claim in `draft`; returns `ClaimDetailResponse`. |
| `list(owner, query, pagination)` | Applies the filters and the clamp; returns `Page<ClaimResponse>` without items. |
| `get(owner, claim_id)` | Loads the claim **and its items**, both owned by `owner`; `404` otherwise. |
| `update(owner, claim_id, request)` | Validates only the fields present; applies a partial update; `404` otherwise. |
| `delete(owner, claim_id)` | Deletes the claim, its items cascade; `404` otherwise. |
| `add_item(owner, claim_id, request)` | Requires `draft`; inserts the item and recomputes the total in one transaction. |
| `update_item(owner, claim_id, item_id, request)` | Requires `draft`; updates the present fields and recomputes the total in one transaction. |
| `delete_item(owner, claim_id, item_id)` | Requires `draft`; deletes the item and recomputes the total in one transaction. |
| `submit(owner, claim_id)` | `draft` → `submitted` after the preconditions; `rejected` → `draft` (reopen only); anything else `409`. |
| `decide(owner, claim_id, decision)` | `submitted` → `approved` or `rejected`; anything else `409`. |
| `pay(owner, claim_id)` | `approved` → `paid`; anything else `409`. |

The `owner` argument is the authenticated `user_id`. It is the first parameter
of every method, which makes it impossible to call a claim method without
saying whose claim is meant.

### 2.2 `ClaimRepository` and `ItemRepository`

The planned statements, all parameterised:

| Method | SQL (sketch) |
| ------ | ------------ |
| `insert_claim(id, user_id, title, description, currency, project_id)` | `INSERT INTO expense_claims (…) VALUES (…) RETURNING …, total_amount::text` |
| `find_owned(claim_id, user_id)` | `SELECT …, total_amount::text FROM expense_claims WHERE id = $1 AND user_id = $2` |
| `list_owned(user_id, filters, limit, offset)` | `SELECT … WHERE user_id = $1 AND ($2::text IS NULL OR status = $2) … ORDER BY created_at DESC, id DESC LIMIT $… OFFSET $…`, plus a matching `COUNT(*)` |
| `update_owned(claim_id, user_id, title, description, project_id)` | `UPDATE expense_claims SET title = COALESCE($3, title), …, updated_at = NOW() WHERE id = $1 AND user_id = $2 RETURNING …` |
| `delete_owned(claim_id, user_id)` | `DELETE FROM expense_claims WHERE id = $1 AND user_id = $2` |
| `insert_item(…)` | `INSERT INTO expense_items (…) VALUES (…) RETURNING …` |
| `find_item_owned(claim_id, item_id, user_id)` | `SELECT … FROM expense_items WHERE id = $1 AND claim_id = $2 AND user_id = $3` |
| `list_items_owned(claim_id, user_id)` | `SELECT … WHERE claim_id = $1 AND user_id = $2 ORDER BY incurred_on, created_at, id` |
| `delete_item_owned(claim_id, item_id, user_id)` | `DELETE FROM expense_items WHERE id = $1 AND claim_id = $2 AND user_id = $3` |
| `recompute_total(claim_id, user_id)` | `UPDATE expense_claims SET total_amount = (SELECT COALESCE(SUM(amount), 0) FROM expense_items WHERE claim_id = $1 AND user_id = $2), updated_at = NOW() WHERE id = $1 AND user_id = $2 RETURNING total_amount::text` |

Three rules that every statement follows:

1. **`user_id` is in the `WHERE` clause, not in a check.** This is
   [`ADR-0005`](../../decisions/ADR-0005-ownership-enforced-in-sql.md).
2. **The item predicate carries both `claim_id` and `user_id`.** An item id
   alone is never enough, so an item cannot be reached by combining an id from
   one account with a claim id from another.
3. **Money is selected with `::text`.** No `numeric` reaches Rust as anything
   but its exact rendering ([`ADR-0007`](../../decisions/ADR-0007-money-representation.md)).

### 2.3 The money type

`dto.rs` declares a newtype, `Money(String)`, with a validating constructor.
It exists so that a money field cannot be deserialised from a JSON number by
accident:

- `Deserialize` accepts only a JSON string; a number is a deserialisation
  failure, which the extractor reports as a `422` naming the field.
- The constructor checks the shape — at most 15 integer digits, a `.`, and
  exactly 4 digits — and rejects a negative value for an amount.
- `Serialize` emits the string unchanged.
- There is no arithmetic method. If one is ever needed, it is a new decision
  ([`ADR-0007`](../../decisions/ADR-0007-money-representation.md)).

### 2.4 The lifecycle guard

One function, `ensure_transition(current, action) -> Result<Next, AppError>`,
holds the transition table, and the service calls it before every state
change. A table with one implementation is what makes "every forbidden
transition is refused" a single testable rule rather than eight `if`
statements.

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
index on `(id, user_id)`; [`schema.md`](../../database/schema.md) records it.

The optional link is `expense_claims.project_id → projects (id) ON DELETE SET
NULL`. Deleting a project clears the reference and leaves the claim.

The physical definition — column types, the `CHECK` constraints, the indexes —
belongs in [`schema.md`](../../database/schema.md) and is written with the
migration. The plan's working sketch is in
[`plan.md`](../../engineering/runs/chain/plan.md) §5.1.

## 4. Invariants

| # | Invariant | Enforced by |
| - | --------- | ----------- |
| 1 | A claim belongs to exactly one user; an item belongs to one claim and to the same user. | The non-null foreign keys, and `user_id` in the item predicate. |
| 2 | A claim or item of another account is invisible and answers `404`. | `user_id = $n` in every statement; `AppError::NotFound` in the service. |
| 3 | `total_amount = SUM(items.amount)`, always. | Recomputed in the same transaction as every item write, with a `CHECK`-able `>= 0`. |
| 4 | One currency per claim, never converted. | `currency` is create-only in the DTO and has no update path. |
| 5 | An amount is non-negative and at most 15 integer digits. | `Money` validation, and a database `CHECK (amount >= 0)` plus the column type. |
| 6 | A claim in any status other than `draft` has at least one item and a positive total. | The submit precondition; a `CHECK` cannot see the items, so this one is a service rule. |
| 7 | Only the listed transitions happen; `paid` is terminal. | `ensure_transition`, and a `status` `CHECK` for the five values. |
| 8 | A partial update leaves an omitted field untouched; `project_id: null` clears the link. | `Option`/`Option<Option<…>>` in the DTO and `COALESCE` plus an explicit null path in SQL. |
| 9 | The list order is total and stable. | `ORDER BY created_at DESC, id DESC`. |
| 10 | The item order is chronological and total. | `ORDER BY incurred_on, created_at, id`. |
| 11 | A receipt is a reference, never bytes. | The column is `text`; no route accepts a file ([`ADR-0009`](../../decisions/ADR-0009-receipt-references.md)). |
| 12 | Deleting a project keeps the claim. | `ON DELETE SET NULL`. |

## 5. Error handling

| Situation | Error | Status |
| --------- | ----- | ------ |
| Title, description, currency, amount, date, category or receipt invalid | `validation_failed` naming the field | `422` |
| Submission with no items or a zero total | `validation_failed` naming `items` or `total_amount` | `422` |
| The claim or item does not exist, or belongs to another account | `not_found` | `404` |
| The optional project belongs to another account | `not_found` | `404` |
| A transition the lifecycle does not allow | `conflict` | `409` |
| An item write on a claim that is not a draft | `conflict` | `409` |
| The body is not JSON, or a path parameter is not a UUID | `bad_request` | `400` |
| A query parameter cannot be parsed, or is outside its enum | `bad_request` | `400` |
| No or invalid credentials | `unauthorized` | `401` |
| A body over the limit | `payload_too_large` | `413` |
| The database failed | `database_error` | `500` |

A missing claim and a claim of another account produce the **same** response.
That is deliberate: a `403` would confirm that the identifier exists.

## 6. The security boundary

```text
                 ┌──────────────────────────────────────────────┐
  bearer token ─►│ /claims, /claims/{claim_id}, /claims/…/items │  authenticated
                 │ /claims/{claim_id}/{submit|approve|reject|pay}│
                 └──────────────────────────────────────────────┘
                                    │
                                    │  user_id from the credential only
                                    ▼
                 every statement carries `user_id = $n`
                 the item statements carry `claim_id` **and** `user_id`
```

- There is no anonymous route in this feature.
- A claim's `project_id` is verified against `projects.user_id` before it is
  stored, so a claim cannot point at another account's project.
- An item id is only meaningful **inside** its claim. The path
  `/claims/{claim_id}/items/{item_id}` is resolved as a pair, so a caller
  cannot move an item between claims by guessing an id.
- The receipt URL is owner data. It is never fetched (no SSRF) and never
  logged (it can carry a signed credential).

## 7. The request path

### 7.1 Adding an item

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

### 7.2 Transitioning

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
| `src/api/routes.rs` | Registers this feature's routes and schemas in the OpenAPI document. | Yes, to be extended. |
| `src/api/extract.rs` | The `Path`, `Query` and `Json` extractors that keep the error envelope intact. | Yes. |
| `src/api/middleware.rs` | `AuthUser`, the source of the ownership predicate. | Yes. |
| `src/pagination.rs` | The list envelope and the clamp. | Yes. |
| `src/helpers/validation.rs` | The shared text validators. | Yes. |
| `src/reimbursements/**` | The feature itself. | **No** |
| `migrations/00NN_expense_claims.sql` | The two tables, their constraints and their indexes. | **No** |

## 9. Related decisions

- [`ADR-0007`](../../decisions/ADR-0007-money-representation.md) — money as
  `NUMERIC(19,4)` and a JSON string.
- [`ADR-0009`](../../decisions/ADR-0009-receipt-references.md) — a receipt is a
  reference, not an upload.
- [`ADR-0005`](../../decisions/ADR-0005-ownership-enforced-in-sql.md) —
  ownership as a SQL predicate.
- [`ADR-0001`](../../decisions/ADR-0001-modular-monolith.md) — why this is a
  module and not a service.

## 10. Limitations of this design

- **The total is a stored column, not a view.** It is recomputed on every item
  write inside the transaction, so a direct SQL edit of an item without the
  recomputation leaves the total stale. The seeder's post-insert verification
  and the feature's tests both check it.
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
- **Deleting a claim is permanent.** There is no soft delete and no history.
