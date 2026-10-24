# Reimbursements — architecture

> **Status:** Implemented
> **Last updated:** 2026-10-24
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-REIMB-001`](specification.md)

How the reimbursement feature is built. The public contract is in
[api.md](api.md); the agreed requirements are in
[specification.md](specification.md).

**Both halves exist.** Migration `0004_expense_claims.sql` creates
`expense_claims`, `expense_items` and the `expense_claim_status` enum;
`src/reimbursements/` holds `mod.rs`, `models.rs`, `dto.rs`, `service.rs`,
`repository.rs` and `handlers.rs`; and all **twelve** published operations are
served under `/api/v1/claims` — five on the claim, three on its items, four
lifecycle actions. What this document describes is what the tree contains, not
what a later chain intends. Where a design decision was taken during the
implementation and differs from the earlier plan, the decision is recorded with
its reason.

## 1. Responsibilities

| Module | Responsibility | Exists |
| ------ | -------------- | ------ |
| [`../../../src/reimbursements/handlers.rs`](../../../src/reimbursements/handlers.rs) | HTTP only: extraction, status codes, response serialization. Twelve handlers with their `utoipa` attributes. | Yes |
| [`../../../src/reimbursements/service.rs`](../../../src/reimbursements/service.rs) | Business rules: validation, the project rule, the transition table, the mapping from a repository outcome to `404`/`409`/`422`. | Yes |
| [`../../../src/reimbursements/repository.rs`](../../../src/reimbursements/repository.rs) | SQL: the claim and item statements and the transition statements, every one scoped by `user_id`; the item transaction; the detail snapshot. | Yes |
| [`../../../src/reimbursements/models.rs`](../../../src/reimbursements/models.rs) | Persistence shapes: `ExpenseClaim`, `ExpenseItem`, `ClaimStatus`. | Yes |
| [`../../../src/reimbursements/dto.rs`](../../../src/reimbursements/dto.rs) | Public contracts: `ClaimResponse`, `ClaimDetailResponse`, `ClaimPage`, `ItemResponse`, `CreateClaimRequest`, `UpdateClaimRequest`, `CreateItemRequest`, `UpdateItemRequest`, `ClaimQuery`, `ClaimsListQuery` and the five length limits. **No money type** — see §2.3. | Yes |

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
| `src/helpers/validation.rs` | The shared text validators, and `validate_money` for the four-decimal input rule. |
| `src/error.rs` (`AppError`) | The one error envelope. |
| `src/config/messages.toml` | Every client-facing string, through the message catalogue. |

It reads `projects` only to verify the optional link, and lets the foreign key do
the clearing. It does not read `notes` or `tasks` at all.

## 2. Interfaces

### 2.1 `ClaimService`

Nine public methods. The four lifecycle routes share one of them, because the
action, not the body, is the input.

| Method | Contract | Exists |
| ------ | -------- | ------ |
| `create(owner, request)` | Validates title, description and currency; verifies the optional project belongs to `owner`; inserts the claim in `draft`; returns `ClaimDetailResponse`. | Yes |
| `list(owner, query, pagination)` | Applies the filters and the clamp; returns `Page<ClaimResponse>` without items. | Yes |
| `get(owner, claim_id)` | Loads the claim **and its items** from one snapshot, both owned by `owner`; `404` otherwise. | Yes |
| `update(owner, claim_id, request)` | Validates only the fields present; applies a partial update; `404` otherwise, naming the project when the project is what failed. | Yes |
| `delete(owner, claim_id)` | Deletes the claim; its items cascade; `404` otherwise. No status guard. | Yes |
| `create_item(owner, claim_id, request)` | Validates the five item fields; requires `draft`; inserts the item and recomputes the total in one transaction; returns `ItemResponse`. | Yes |
| `update_item(owner, claim_id, item_id, request)` | Requires `draft`; updates the present fields and recomputes the total in one transaction; `404` when the item is not under that claim. | Yes |
| `delete_item(owner, claim_id, item_id)` | Requires `draft`; deletes the item and recomputes the total in one transaction. | Yes |
| `transition(owner, claim_id, action)` | One guarded `UPDATE` for the action; on zero rows, `explain_refusal` maps the state to `404`, `409` or the `422` of a refused submit; returns `ClaimDetailResponse` read after the write. | Yes |

The `owner` argument is the authenticated `user_id`. It is the first parameter
of every method, which makes it impossible to call a claim method without
saying whose claim is meant. `AppState` builds the service once, from the pool,
the message catalogue and the configured pagination, and the field is
`state.reimbursements`.

`list` returns `Page<ClaimResponse>`, not `Page<ClaimDetailResponse>`: the
listing does not carry items. The item and lifecycle methods return a single
resource, never a page.

### 2.2 `ClaimRepository`

Twelve methods over **nineteen statement constants**: ten for the claim — the
insert, the single read, the item read, the two list shapes, the two counts,
the update, the delete and the detail snapshot's `SET TRANSACTION` — six for
the items (the lock, the insert, the update, the delete, the total recompute
and the item count) and three for the lifecycle. A request runs one, two or
three of them; §2.6 and §2.7 say which.

| Method | What the statement does | Ownership predicate |
| ------ | ----------------------- | ------------------- |
| `insert` | `INSERT … SELECT … WHERE <project is null> OR EXISTS (SELECT 1 FROM projects WHERE id = $n AND user_id = $owner) RETURNING …`. Writes `'draft'` explicitly; `total_amount` comes from the column default and renders `"0.0000"`. | The `EXISTS` names the caller. `user_id` is written from the token, never from the request. |
| `find_owned` | The single-row read of a claim. | `id = $1 AND user_id = $2` |
| `list_items_owned` | Reads the items of one claim in the published order. | `claim_id = $1 AND user_id = $2` — both, always |
| `find_detail_owned` | The claim and its items in **one** `REPEATABLE READ, READ ONLY` transaction, so the total and the items of one response always agree. | `id = $1 AND user_id = $2` and `claim_id = $1 AND user_id = $2` |
| `update_owned` | The partial update: `COALESCE($n, column)` per field, an explicit `CASE` for the one field that can be set to `NULL`, `updated_at = NOW()`, and the project rule in the same `WHERE`. | `id = $1 AND user_id = $2`, plus `user_id = $2` inside the `EXISTS` over `projects` |
| `delete_owned` | One `DELETE`, with **no status predicate**; the items follow through `ON DELETE CASCADE`. | `id = $1 AND user_id = $2` |
| `list_owned` | The page and its count, in two shapes — §2.5. | `user_id = $1` on both |
| `count_items_owned` | `COUNT(*)` of a claim's items. Used only on the error path of `submit`, to say which field the refusal names. | `claim_id = $1 AND user_id = $2` |
| `insert_item` | Begin; `SELECT status … FOR UPDATE`; `INSERT`; recompute; commit. §2.7. | `claim_id = $1 AND user_id = $2` on the lock and the recompute; `user_id` written from the token on the insert |
| `update_item` | Begin; lock; `UPDATE … WHERE id AND claim_id AND user_id`; recompute; commit. | all three |
| `delete_item` | Begin; lock; `DELETE … WHERE id AND claim_id AND user_id`; recompute; commit. | all three |
| `transition_owned` | One guarded `UPDATE` for the action. §2.6. | `id = $1 AND user_id = $2`, plus the expected `status` |

Five rules the statements follow:

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
3. **The item predicate carries `id`, `claim_id` and `user_id`.** An item id is
   only meaningful inside its claim, so an id from one account cannot be
   combined with a claim id from another, and an id under a different claim of
   the **same** owner matches no row.
4. **Money is selected with `::text` and an explicit alias.** No `numeric`
   reaches Rust as anything but its exact rendering
   ([`ADR-0007`](../../decisions/ADR-0007-money-representation.md)). A money
   column selected without `::text` does not decode at all, so the mistake fails
   loudly instead of publishing a number.
5. **Timestamps written by a transition or an item write use
   `clock_timestamp()`, not `NOW()`.** `NOW()` is the transaction's start
   instant; for a transaction that waited on the claim row lock it can predate
   the `submitted_at` it is compared against, and
   `expense_claims_timestamp_order_chk` would then reject a correct approval.

`update_owned` needs its second statement only on the failure path, and so does
`transition_owned`. That is a deliberate trade, recorded as a limitation in §10.

### 2.3 Money: a string, not a type

**There is no `Money` newtype.** The design that settled this is
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
3. The validator that replaced it is one function beside the other shared text
   validators, which is where the module-admission rule in
   [`../../../src/helpers/mod.rs`](../../../src/helpers/mod.rs) expects such a
   rule to live.

What exists instead:

- Every money field is `String` in the DTO and comes from a `::text` select.
  `NUMERIC(19,4)::text` renders exactly four decimals (`"149.8000"`,
  `"0.0000"`), which **is** the wire form; serde quotes it as a JSON string.
- No `f32`, no `f64`, no decimal crate, and no arithmetic in Rust. The total is
  computed by `SUM` in SQL, and nowhere else.
- The four-decimal **input** rule is
  `helpers::validation::validate_money`, built in the item wave: it accepts
  `^[0-9]{1,15}\.[0-9]{4}$` and returns the string unchanged, and it is applied
  to `CreateItemRequest.amount` and to `UpdateItemRequest.amount` when present.
  It is a character check, not a parse: nothing rounds, compares or sums there.

The database column is permissive on **input** and exact on **output**: it
coerces `"12.5"` to `12.5000` and rounds a fifth decimal instead of refusing it.
That is why the input rule cannot be left to the column, and why
`money_is_stored_with_four_decimal_places` in
[`../../../tests/claims_schema.rs`](../../../tests/claims_schema.rs) records the
behaviour rather than treating the scale as validation.

### 2.4 The nullable fields are tri-states

`project_id` is the claim field whose "absent" and "null" mean different
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

**The item's two nullable fields reuse the same seam**, through
`double_option_string` and two accessors of their own:

| Field | Accessor | `null` means |
| ----- | -------- | ------------ |
| `category` | `UpdateItemRequest::category_change() -> (bool, Option<&str>)` | clear the column |
| `receipt_url` | `UpdateItemRequest::receipt_url_change() -> (bool, Option<&str>)` | clear the reference |

`an_item_category_and_receipt_are_kept_cleared_or_set_by_the_three_body_forms`
deserialises the three body forms and asserts both accessors. An **empty**
category is the published "store `null`" case, so on a `PATCH` it is also a
clear — `resolve_category` in the service turns it into the same pair.

`title` and `description` have no tri-state: `{"title": null}` is
indistinguishable from an omitted key and is treated as omitted, `{"title": ""}`
is a `422`, and `{"description": ""}` is the one empty string that means
something — it clears the description, which is the column's own default. The
same rule holds for an item: `{"description": ""}` is a `422`, because
`expense_items_description_chk` has a lower bound of one character and no
"clear" spelling.

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
`expense_claims_project_id_idx` is not read by the published listing, which has
no `project_id` filter.

### 2.6 The lifecycle guard — built

The table has **five edges** and twenty combinations:

| From | Action | To | Timestamp written |
| ---- | ------ | -- | ----------------- |
| `draft` | `submit` | `submitted` | `submitted_at` |
| `submitted` | `approve` | `approved` | `decided_at` |
| `submitted` | `reject` | `rejected` | `decided_at` |
| `approved` | `pay` | `paid` | `paid_at` |
| `rejected` | `submit` | `draft` | none; the reopen **clears `submitted_at` and `decided_at`** |

Every other pair — fifteen of them — is refused, and `paid` is terminal.

`ensure_transition(current, action) -> Option<ClaimStatus>` in `service.rs` is
the table's single definition in Rust, and the unit test
`the_transition_table_allows_exactly_the_five_edges` walks all twenty
combinations against it.

**The table is not on the success path.** A lifecycle action is **one guarded
`UPDATE`**, not a read followed by a write:

```text
UPDATE expense_claims
   SET status = …, <the stamp the CHECK requires> = clock_timestamp(), updated_at = clock_timestamp()
 WHERE id = $1 AND user_id = $2 AND status = <the expected source status>
RETURNING …
```

That is what makes two concurrent actions safe. The second statement blocks on
the row lock, then re-evaluates its own `WHERE` against the committed row, and
because the status has already moved it matches nothing: exactly one of the two
actions wins, and the loser gets `409`. A check-then-write would let both read
`submitted` and both decide.

A zero-row result is ambiguous — absent, foreign, wrong status, or a refused
submit precondition — so the service explains it **after** the write has already
refused, in this order:

1. the claim does not exist for this caller → `404 not_found`;
2. `ensure_transition(claim.status, action)` is `None` → `409 conflict`
   (`transition_not_allowed`);
3. the table allows the pair, so for `submit` from `draft` the refusal is the
   precondition: no items → `422` naming `items`, items but a zero total →
   `422` naming `total_amount` (`REIMB-R05`);
4. otherwise a concurrent action moved the claim between the statement and this
   read → `409 conflict`.

The submit statement carries the precondition itself, so no request path can
skip it:

```text
AND (status = 'rejected'
     OR (total_amount > 0
         AND EXISTS (SELECT 1 FROM expense_items WHERE claim_id = $1 AND user_id = $2)))
```

`submit` covers two edges in one statement because the action is ambiguous about
its source state; the `CASE` arms are the two rows of the table for that action.
The draft arm keeps `decided_at` and `paid_at`, which the lifecycle constraint
already requires to be null for a draft. The reopen arm nulls all three, because
`expense_claims_lifecycle_timestamps_chk` requires a `draft` to carry no stamp
at all — clearing one and not the other would be a `23514` at commit instead of
a clean `200`. `approve` and `reject` share one statement that binds the target
status; `pay` has its own.

**The table exists twice** — as `ensure_transition` in Rust and as the `status`
predicates of the three statements — and nothing at compile time keeps them in
step. What keeps them in step is the test pair: the twenty-case unit test for
the Rust table and `every_forbidden_transition_is_refused` plus
`a_draft_can_be_submitted_approved_and_paid` for the SQL. A sixth edge would
have to be added in both, and the tests are what say so. This is recorded as a
limitation in §10.

### 2.7 The item transaction and the total invariant

Every item write is **one transaction, in this order**, and the order is the
invariant:

```text
BEGIN
  ├─ SELECT status FROM expense_claims
  │    WHERE id = $1 AND user_id = $2 FOR UPDATE     no row   → 404
  │                                                  not draft → 409
  ├─ INSERT / UPDATE / DELETE the item               no row on update or
  │                                                  delete   → 404
  ├─ UPDATE expense_claims
  │    SET total_amount = COALESCE(
  │          (SELECT SUM(amount) FROM expense_items
  │            WHERE claim_id = $1 AND user_id = $2), 0),
  │        updated_at = clock_timestamp()
  │    WHERE id = $1 AND user_id = $2
COMMIT
```

Four properties follow from the shape, not from a later check:

1. **The item and the total cannot diverge.** The write and the recompute are in
   the same transaction, so either both commit or neither does. A failure after
   the insert leaves neither the item nor a new total.
2. **`FOR UPDATE` serialises the writers of one claim.** Two concurrent inserts
   would otherwise each compute a `SUM` that misses the other's row; the lock
   forces the second to wait, and its `SUM` therefore sees the first's committed
   item. Item writes on one claim are consequently serialised — the throughput
   ceiling of the feature, and irrelevant for a personal workspace.
3. **`SUM` over no rows is `NULL`, hence the `COALESCE`.** Deleting the last item
   stores `0`, which renders `"0.0000"`, never `null` and never a stale total.
4. **The claim's `updated_at` moves with its items.** The recompute sets it, so
   a claim's modification time is the last change to the claim *or* to one of
   its items. That is deliberate: the response is a detail, and a client
   polling `updated_at` should see the item write.

The refusal paths return before any write, and the transaction is dropped rather
than committed, so a refused item write leaves nothing behind. The repository
never builds an error: it returns `ItemWrite::{Applied, ClaimNotFound,
ClaimNotDraft, ItemNotFound}`, and the service maps each variant to its
catalogue message and status, so a new outcome cannot be silently ignored.

**The detail read is one snapshot.** `find_detail_owned` runs the claim read and
the item read inside a `REPEATABLE READ, READ ONLY` transaction. Under the
default `READ COMMITTED` those were two snapshots, so an item committed between
them would be published next to a `total_amount` that predates it — exactly the
divergence the item transaction exists to prevent. The cost is one extra
statement (`SET TRANSACTION …`) and a transaction per detail read, `GET`,
`PATCH` and every lifecycle response all use it.

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

`atom-seed` seeds claims and items, because its count check compares its own
table list against `information_schema` and every run would otherwise fail. The
profiles are **1 / 25 / 50 claims** and **0 / 48 / 98 items** for `minimal` /
`dev` / `demo`. Claim `i` gets `1 + (i % 3)` items **except claim `0`**, the
profile's first claim, which is a `draft` (`status` cycles the five states by
`i % 5`) and gets none — that single exception is why `minimal` seeds no items
at all. `currency` cycles a fixed three, and `submitted_at`, `decided_at` and
`paid_at` are set so that `expense_claims_lifecycle_timestamps_chk` holds for
every row. `total_amount` is the `SUM` of the claim's items, computed in SQL.
The seeder asserts the counts exactly and re-reads every seeded claim's total
against its items; `tests/claims_schema.rs` owns the constraint proof, the
seeder owns the data.

The API can now reach all five statuses and any total, so the endpoint tests no
longer need SQL to build a lifecycle state: `claim_in_status` in
`tests/claims_api.rs` creates a claim and walks it with the real action routes,
which means a lifecycle that cannot reach one of its own states fails there
rather than quietly making a transition test vacuous. Two cases still use a SQL
fixture — the listing filters with fixed `created_at` instants, and the item
order — and every fixture is scoped to the account the test registered, so none
of the seeder's data is needed to run the API suite.

## 4. Invariants

| # | Invariant | Enforced by | Exists |
| - | --------- | ----------- | ------ |
| 1 | A claim belongs to exactly one user; an item belongs to one claim and to the same user. | The non-null foreign keys, the composite item key, and `user_id` in every item predicate. | Yes |
| 2 | A claim or item of another account is invisible and answers `404`. | `user_id = $n` in every statement; `AppError::NotFound` in the service. | Yes |
| 3 | `total_amount = SUM(items.amount)`, always. | Recomputed by `SUM` in SQL in the same transaction as every item write, after the claim row is locked. | Yes, for every API path (§2.7) |
| 4 | One currency per claim, never converted. `currency` is required and upper-cased on create and is **not patchable**: a `PATCH` body carrying it — or `status`, or `total_amount` — is **accepted with `200` and the field is ignored**, because none of the three is a field of `UpdateClaimRequest` and none appears in any `SET` clause. There is no `422` for them; that is the published `api.md` §6 behaviour, and [status.md](status.md) §1.2 records why the earlier "service rule with a `422`" wording was wrong. | The create validator, the three-field `UpdateClaimRequest`, and the `SET` clause of `update_owned`. Asserted by `the_claim_currency_is_not_patchable` and `the_claim_status_is_not_patchable`. | Yes |
| 5 | An amount is non-negative and at most 15 integer digits, with exactly four decimals. | `validate_money` for input, `expense_items_amount_chk` and `NUMERIC(19,4)` at rest. | Yes |
| 6 | A claim in any status other than `draft` has at least one item and a positive total. | The `WHERE` of the submit statement (§2.6); a `CHECK` cannot see the items, so this one is SQL inside the request, not a constraint. | Yes, for the request path |
| 7 | Only the listed transitions happen; `paid` is terminal. | The `status` predicate of the three transition statements, `ensure_transition`, and the `expense_claim_status` enum for the five values. | Yes |
| 8 | A partial update leaves an omitted field untouched; `project_id: null` clears the link; `category: null` and `receipt_url: null` clear those columns; an empty body changes only `updated_at`. | `Option`/`Option<Option<…>>` in the DTOs (§2.4) and `COALESCE` plus an explicit `null` path in SQL. | Yes |
| 9 | The list order is total and stable. | `ORDER BY created_at DESC, id DESC`. | Yes |
| 10 | The item order is chronological and total. | `ORDER BY incurred_on, created_at, id`. | Yes |
| 11 | A receipt is a reference, never bytes. | The column is `text`; no route accepts a file ([`ADR-0009`](../../decisions/ADR-0009-receipt-references.md)). | Yes |
| 12 | Deleting a project keeps the claim. | `ON DELETE SET NULL`. | Yes |

The migration enforces the **data** half of invariants 1, 4, 5, 11 and 12: the
two `user_id` keys and the composite item key, `expense_claims_currency_chk`,
`expense_items_amount_chk`, the `receipt_url` column type, and `ON DELETE SET
NULL`. Invariants 2, 3, 6, 7, 8 and 9 are held by the statements as well. A
constraint holds data; it does not answer a request, and that is why the two
halves are listed separately.

## 5. Error handling

| Situation | Error | Status | Exists |
| --------- | ----- | ------ | ------ |
| Title, description or currency invalid, on create | `validation_failed` naming the field | `422` | Yes |
| `search` over its bound, `currency` filter not three letters, reversed date range | `validation_failed` naming the field | `422` | Yes |
| Item description, amount, date, category or receipt invalid | `validation_failed` naming the field | `422` | Yes |
| A body value of the wrong type — including a JSON number where money is expected | `validation_failed`, field `body` | `422` | Yes |
| Submission with no items or a zero total | `validation_failed` naming `items` or `total_amount` | `422` | Yes |
| The claim does not exist, or belongs to another account | `not_found` | `404` | Yes |
| The optional project does not exist, or belongs to another account | `not_found` | `404` | Yes |
| The item does not exist, does not belong to that claim, or belongs to another account | `not_found` | `404` | Yes |
| A transition the lifecycle does not allow | `conflict` (`transition_not_allowed`) | `409` | Yes |
| An item write on a claim that is not a draft | `conflict` (`claim_not_editable`) | `409` | Yes |
| The body is not JSON, or a path parameter is not a UUID | `bad_request` | `400` | Yes |
| A query parameter cannot be parsed, or is outside its enum | `bad_request` | `400` | Yes |
| A missing required field | `validation_failed`, field `body` | `422` | Yes |
| No or invalid credentials | `unauthorized` | `401` | Yes |
| A body over the limit | `payload_too_large` | `413` | Yes |
| The database failed | `database_error` | `500` | Yes |
| The request exceeded the configured timeout | `timeout` | `408` | Yes, from the middleware |

Two rules the service follows, in this order:

1. **Validate the whole body first, then touch the database.** A request that is
   both malformed and aimed at a foreign resource is a `422`, not a `404`.
2. **Every repository miss becomes `NotFound` with a catalogue message.** The
   service never compares identifiers and never returns `403` for a resource
   ([`ADR-0005`](../../decisions/ADR-0005-ownership-enforced-in-sql.md) §3).

A missing claim and a claim of another account produce the **same** response.
That is deliberate: a `403` would confirm that the identifier exists.

The `409` is decided by the statement, not by the service: the service only
translates a zero-row outcome. That ordering matters for a concurrent caller —
if the service decided, two callers could both pass its check.

## 6. The security boundary

```text
                 ┌──────────────────────────────────────────────┐
  bearer token ─►│ /claims, /claims/{claim_id}                  │  served (R2)
                 │ /claims/{claim_id}/items                     │  served (R3)
                 │ /claims/{claim_id}/{submit|approve|reject|pay}│  served (R4)
                 └──────────────────────────────────────────────┘
                                    │
                                    │  user_id from the credential only
                                    ▼
                 every statement carries `user_id = $n`
                 the item statements carry `item_id`, `claim_id` **and** `user_id`
```

- There is no anonymous route in this feature. `the_claim_routes_require_a_credential`
  calls all twelve with no token and with an invalid one.
- A claim's `project_id` is verified against `projects.user_id` **inside the
  write statement**, so a claim cannot point at another account's project and
  the check cannot be skipped by a caller who sends the field late.
- An item id is only meaningful **inside** its claim. The path
  `/claims/{claim_id}/items/{item_id}` is resolved as a triple — item, claim and
  owner — so a caller cannot move an item between claims by guessing an id, and
  cannot reach another account's item by combining identifiers.
- The receipt URL is owner data. It is never fetched (no SSRF) and never logged
  (it can carry a signed credential).
- `user_id` is never accepted from a request body and never appears in a
  response: the DTOs drop it, and the statements write it from the token.
- A lifecycle action is authorised by the same `user_id` predicate as a read.
  `a_lifecycle_action_on_another_users_claim_is_not_found` proves all four
  actions answer `404` and write nothing.

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
  │    └─ 200 OK, ClaimDetailResponse (one repeatable-read snapshot)
```

An ignored field never reaches the service: `currency`, `status` and
`total_amount` are not fields of `UpdateClaimRequest` (§4 invariant 4). A body
with no field at all is still a `200`, and the only change is `updated_at`.

### 7.2 Adding an item — served

```text
POST /api/v1/claims/{claim_id}/items
  │
  ├─ api::middleware::AuthUser → user_id
  ├─ api::extract::Path<Uuid>                           not a UUID → 400
  ├─ api::extract::Json<CreateItemRequest>              not JSON → 400
  │                                                     wrong type → 422, field `body`
  ├─ reimbursements::service::ClaimService::create_item
  │    ├─ validate description, amount, category, receipt_url
  │    │                                                → 422 naming the field
  │    ├─ BEGIN
  │    │    ├─ SELECT status … FOR UPDATE               no row   → 404
  │    │    │                                           not draft → 409
  │    │    ├─ INSERT INTO expense_items …
  │    │    └─ UPDATE expense_claims SET total_amount = SUM(…)
  │    └─ COMMIT
  └─ 201 Created, ItemResponse
```

The response is the **item**, not the claim: the published `api.md` §8 says the
claim's new total is read with `GET /claims/{claim_id}`, and the tests do exactly
that, so a stale total cannot hide behind the mutating response.

`PATCH` and `DELETE` on an item follow the same three steps. A `PATCH` that
matches no item under that claim is the item's `404`; a `DELETE` with
`rows_affected() == 0` is the same.

### 7.3 Transitioning — served

```text
POST /api/v1/claims/{claim_id}/approve
  │
  ├─ AuthUser → user_id
  ├─ Path → claim_id                                    not a UUID → 400
  ├─ reimbursements::service::ClaimService::transition(…, Approve)
  │    ├─ UPDATE … SET status = 'approved', decided_at = clock_timestamp()
  │    │     WHERE id = $1 AND user_id = $2 AND status = 'submitted'
  │    │       1 row  → continue
  │    │       0 rows → explain_refusal  (§2.6)
  │    │                404 → 409 → 422 (submit only) → 409
  │    └─ 200 OK, ClaimDetailResponse in the new status
```

`submit` has no timestamp of its own in this sketch because its statement writes
one of two: `submitted_at` for the draft edge, and `NULL` for the reopen's
`submitted_at` and `decided_at`.

## 8. Related files

| File | Role | Exists |
| ---- | ---- | ------ |
| [`../../../src/api/routes.rs`](../../../src/api/routes.rs) | Registers the twelve claim routes, the twelve `ROUTE_TABLE` rows and the nine component schemas in the OpenAPI document. | Yes |
| [`../../../src/api/extract.rs`](../../../src/api/extract.rs) | The `Path`, `Query` and `Json` extractors that keep the error envelope intact. | Yes |
| [`../../../src/api/middleware.rs`](../../../src/api/middleware.rs) | `AuthUser`, the source of the ownership predicate. | Yes |
| [`../../../src/pagination.rs`](../../../src/pagination.rs) | The list envelope and the clamp. | Yes |
| [`../../../src/helpers/validation.rs`](../../../src/helpers/validation.rs) | The shared text validators and `validate_money`. | Yes |
| [`../../../src/reimbursements/`](../../../src/reimbursements/) | The feature itself. | All five modules |
| [`../../../migrations/0004_expense_claims.sql`](../../../migrations/0004_expense_claims.sql) | The two tables, their constraints and their indexes. | Yes |
| [`../../../tests/claims_schema.rs`](../../../tests/claims_schema.rs) | The constraint, cascade and money-scale proofs. | Yes |
| [`../../../tests/claims_api.rs`](../../../tests/claims_api.rs) | The 55 endpoint tests: 34 on the claim, 21 on the items and the lifecycle. | Yes |

## 9. Related decisions

- [`ADR-0007`](../../decisions/ADR-0007-money-representation.md) — money as
  `NUMERIC(19,4)` and a JSON string.
- [`ADR-0009`](../../decisions/ADR-0009-receipt-references.md) — a receipt is a
  reference, not an upload.
- [`ADR-0005`](../../decisions/ADR-0005-ownership-enforced-in-sql.md) —
  ownership as a SQL predicate.
- [`ADR-0001`](../../decisions/ADR-0001-modular-monolith.md) — why this is a
  module and not a service.

No new ADR was written for `R2`, `R3` or `R4`. The twelve endpoints fit the
accepted model, and the two structural decisions — no money type, with the check
a validator in `helpers`; and a stored total recomputed by `SUM` inside the item
transaction — narrow a plan in
[`module-boundaries.md`](../../architecture/module-boundaries.md) rather than
reversing a decision. A change that needs money handled in Rust, or the total
computed outside SQL, needs an ADR superseding `ADR-0007`.

## 10. Limitations of this design

- **The total is a stored column, not a view.** Every API path now maintains it
  inside the transaction, so no request can produce a stale total. A direct SQL
  edit of an item still can, because it bypasses the recompute. The seeder's
  post-insert verification checks it on seeded data; no HTTP test can produce
  the divergence, which is why there is none.
- **The submit precondition is not a database constraint.** A `CHECK` cannot
  count the items of another table, so "at least one item and a positive total"
  lives in the `WHERE` of the submit statement. A direct SQL status write can
  therefore create a `submitted` claim with no items.
- **The transition table is written twice.** `ensure_transition` in Rust and the
  three statements' `status` predicates express the same five edges. Nothing at
  compile time links them; the twenty-case unit test and the fifteen-case HTTP
  test are what would catch a drift. `ensure_transition` is only on the error
  path, so it cannot be the guard.
- **The item `user_id` is redundant.** It is an intentional denormalisation,
  kept so that the item statements carry the ownership predicate without a
  join. The composite foreign key on `(claim_id, user_id)` makes a
  disagreement unrepresentable; the cost is the extra unique index on the
  parent.
- **One transaction per item write, one claim row lock.** Two concurrent item
  writes on the same claim serialise. That is what makes the `SUM` correct and
  it is also the throughput ceiling of the feature. No test runs two writers at
  once, so the serialisation is argued from the lock, not measured.
- **The lifecycle is a service rule, not a state machine in the database.** The
  guard is the `WHERE` of a statement the service issues; a `status` change
  written directly by SQL bypasses it.
- **A `PATCH` with a foreign `project_id` costs one extra owner-scoped read on
  the failure path**, to name the resource correctly. The success path is one
  statement. A refused lifecycle action costs one read (and, for `submit`, one
  count) on the same path for the same reason.
- **The detail read costs a transaction.** `GET`, `PATCH` and the four
  lifecycle responses read the claim and its items from one `REPEATABLE READ`
  snapshot, which is one extra statement and one commit per read.
  Consistency was chosen over the round trip
  ([`../../architecture/reimbursements-crud-design.md`](../../architecture/reimbursements-crud-design.md)
  §6.7).
- **`?currency=` is a `422`, while `?search=` means "no filter".** Each follows
  the published sentence for that parameter; the difference is in the contract,
  not in the code.
- **A `%` or `_` in `search` is a wildcard.** Identical to `notes` and
  `projects`; fixing it in one feature would make two searches behave
  differently.
- **Deleting a claim is permanent.** There is no soft delete and no history, in
  any status (`REIMB-R11`).
