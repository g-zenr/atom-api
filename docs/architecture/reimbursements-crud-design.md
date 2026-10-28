# Reimbursements claim CRUD — design for `R2`

> **Status:** Proposed (design for the chain `D2.R2`; **not built**)
> **Date:** 2026-10-24
> **Owner:** `architect`
> **Chain:** `D2.R2` — claim CRUD. Contract `C-R1` is done; `C-R2` is this
> document turned into a contract.

This is the design of the **claim** half of the reimbursement API: list, create,
read, update and delete. It is written before the code, so **nothing in it is
`Implemented` or `Verified`**. Every file, type, function and statement it names
under `src/reimbursements/` is a proposal for `R2` except `mod.rs` and
`models.rs`, which exist.

- The published contract is [`../features/reimbursements/api.md`](../features/reimbursements/api.md).
  Where this document and that one disagree, the disagreement is listed in §9
  and resolved in favour of the published document, as
  [`../../AGENTS.md`](../../AGENTS.md) §4.10 requires.
- The schema is [`../database/schema.md`](../database/schema.md) §11.1–§11.2,
  implemented by [`../../migrations/0004_expense_claims.sql`](../../migrations/0004_expense_claims.sql).
  It exists and is covered by [`../../tests/claims_schema.rs`](../../tests/claims_schema.rs);
  this design does not re-check in Rust anything that file proves in SQL.
- The layering, the ownership rule and the error mapping are the Phase 1 ones:
  [`module-boundaries.md`](module-boundaries.md) §1–§7,
  [`ADR-0005`](../decisions/ADR-0005-ownership-enforced-in-sql.md) and
  [`../api/error-contract.md`](../api/error-contract.md).

**Out of scope, deliberately.** Items (`R3`), the lifecycle actions (`R4`), the
cross-feature flow tests (`R5`) and the review/close chain (`R6`). Those are
named only where this design has to leave a boundary for them. Nothing here
designs an item endpoint, a `POST …/submit`, an `ensure_transition`, a
`CreateItemRequest` or a `409` on a claim.

---

## 1. What exists today, and what `R2` adds

| Layer | File | Today | `R2` |
| ----- | ---- | ----- | ---- |
| module | [`../../src/reimbursements/mod.rs`](../../src/reimbursements/mod.rs) | declares `models` | declares `dto`, `handlers`, `repository`, `service` |
| models | [`../../src/reimbursements/models.rs`](../../src/reimbursements/models.rs) | `ExpenseClaim`, `ExpenseItem`, `ClaimStatus` | unchanged |
| dto | `src/reimbursements/dto.rs` | absent | the four response shapes, the two requests, the two query types, the length constants |
| repository | `src/reimbursements/repository.rs` | absent | nine static statements behind six methods, all owner-scoped |
| service | `src/reimbursements/service.rs` | absent | validation, the project rule, the repository-miss to `404` mapping |
| handlers | `src/reimbursements/handlers.rs` | absent | five handlers with their `utoipa` attributes |
| state | [`../../src/state.rs`](../../src/state.rs) | — | `pub reimbursements: ClaimService` |
| routes | [`../../src/api/routes.rs`](../../src/api/routes.rs) | — | five `ROUTE_TABLE` rows, five routes, five `paths(...)`, seven components, one tag |
| catalogue | [`../../config/messages.toml`](../../config/messages.toml), [`../../src/config/messages.rs`](../../src/config/messages.rs) | — | three client-facing strings |

`R2` changes **no** migration, **no** index and **no** table. It adds **no**
dependency and **no** setting (§11).

`R2` also introduces **no transaction**. Create, update and delete are one
statement each; the claim detail read is two reads of one owner (see §6.7 for
the snapshot caveat this leaves to `R3`).

---

## 2. The endpoint set

Five operations, all authenticated by the bearer token, all under
`API_PREFIX` (`/api/v1`, declared once in
[`../../src/api/routes.rs`](../../src/api/routes.rs)). `ROUTE_TABLE` gains the
same five rows, in this order, relative to the prefix.

| # | Method | Path | Request | Success | Response schema |
| - | ------ | ---- | ------- | ------- | --------------- |
| 1 | `GET` | `/api/v1/claims` | query (§7) | `200 OK` | `ClaimPage` |
| 2 | `POST` | `/api/v1/claims` | `CreateClaimRequest` | `201 Created` | `ClaimDetailResponse` |
| 3 | `GET` | `/api/v1/claims/{claim_id}` | path `claim_id: Uuid` | `200 OK` | `ClaimDetailResponse` |
| 4 | `PATCH` | `/api/v1/claims/{claim_id}` | path + `UpdateClaimRequest` | `200 OK` | `ClaimDetailResponse` |
| 5 | `DELETE` | `/api/v1/claims/{claim_id}` | path `claim_id: Uuid` | `204 No Content` | — |

There is no anonymous route, and no `Location` header: the Phase 1 handlers
return `(StatusCode::CREATED, Json(…))` and the convention document
([`../api/conventions.md`](../api/conventions.md) §3) asks for none.

### 2.1 Errors, per operation

`408 timeout` and `500 database_error` are produced by the shared layers and are
possible on all five: `408` by `timeout_middleware` in
[`../../src/api/mod.rs`](../../src/api/mod.rs), `500` by `AppError::Database`
from any statement. They are listed once here rather than five times.
`429` is **not** possible: the rate limiter guards the authentication endpoints
only, and this feature adds no limiter.

| # | Operation | Other statuses and codes |
| - | --------- | ------------------------ |
| 1 | `GET /claims` | `400 bad_request` — `limit`/`offset` not integers, `status` not one of the five values, a date not `YYYY-MM-DD`. `401 unauthorized`. `422 validation_failed` — `search` over 200 characters, `currency` not three letters, `submitted_from` later than `submitted_to`. |
| 2 | `POST /claims` | `400 bad_request` — the body is not JSON. `401 unauthorized`. `404 not_found` — `project_id` does not exist or is not the caller's. `408`, `413 payload_too_large`. `422 validation_failed` — `title`, `description` or `currency` breaks §2.7 of the contract, `project_id` is not a UUID, or a required field is missing. `500`. |
| 3 | `GET /claims/{claim_id}` | `400 bad_request` — `claim_id` is not a UUID. `401`. `404 not_found` — absent or another owner's. `408`, `500`. |
| 4 | `PATCH /claims/{claim_id}` | `400 bad_request` — the body is not JSON, or `claim_id` is not a UUID. `401`. `404 not_found` — the claim, or the project, is absent or another owner's. `408`, `413`. `422 validation_failed` — `title`, `description` or the type of `project_id`. `500`. |
| 5 | `DELETE /claims/{claim_id}` | `400 bad_request` — `claim_id` is not a UUID. `401`. `404 not_found`. `408`, `500`. |

No `409` is possible in `R2`: a claim's metadata is editable in every status in
the published contract (`REIMB-R06` restricts **item** writes to `draft`, which
is `R3`), and nothing in `R2` changes the status. A `409` on a claim first
appears in `R4`.

### 2.2 The DTOs

Mirrors [`../features/reimbursements/api.md`](../features/reimbursements/api.md)
§2.2–§2.5 exactly. `user_id` is on the row and **not** on the wire: the response
types are built from the models and drop it, as `ProjectResponse` drops its
`user_id` today.

```rust
// src/reimbursements/dto.rs (proposed)

pub const MAX_TITLE_LENGTH: usize = 200;        // expense_claims_title_length_chk
pub const MAX_DESCRIPTION_LENGTH: usize = 5_000; // expense_claims_description_chk

pub struct ClaimResponse {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub currency: String,
    pub total_amount: String,          // money, §4
    pub status: ClaimStatus,
    pub project_id: Option<Uuid>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub decided_at: Option<DateTime<Utc>>,
    pub paid_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct ClaimDetailResponse {       // the same fields, flat, plus items
    /* … ClaimResponse's twelve fields … */
    pub items: Vec<ItemResponse>,
}

pub struct ItemResponse {
    pub id: Uuid,
    pub claim_id: Uuid,
    pub description: String,
    pub amount: String,                // money, read by R2, written by R3
    pub incurred_on: NaiveDate,
    pub category: Option<String>,
    pub receipt_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct ClaimPage {
    pub items: Vec<ClaimResponse>,
    pub pagination: Pagination,        // crate::pagination::Pagination
}

pub struct CreateClaimRequest {
    pub title: String,
    pub description: String,           // #[serde(default)] — "" when absent
    pub currency: String,
    pub project_id: Option<Uuid>,
}

pub struct UpdateClaimRequest {
    pub title: Option<String>,         // absent = untouched
    pub description: Option<String>,   // absent = untouched; "" clears
    pub project_id: Option<Option<Uuid>>, // absent = untouched, null = clear; serde in §5.2
}
```

`ClaimDetailResponse` is **flat** — the twelve `ClaimResponse` fields and `items`
side by side — because that is the published JSON. It is not
`{ claim: …, items: … }`.

Three construction rules:

- `impl From<ExpenseClaim> for ClaimResponse`, consuming the row so a listed page
  moves its strings instead of copying them (the `projects` idiom).
- `impl ClaimDetailResponse { pub fn new(claim: ExpenseClaim, items: Vec<ExpenseItem>) -> Self }`.
- `impl From<ExpenseItem> for ItemResponse` drops `user_id`.

`ClaimStatus` is reused from `models.rs`. It already derives `Serialize`,
`Deserialize`, `ToSchema` and `sqlx::Type` with `snake_case` naming, so it is
the *same* enum in a filter, in a response and in the database — one
definition, no second copy in `dto.rs`.

### 2.3 Handlers

One `Query<…>` extractor per list handler, one `Path` and one `Json` where the
route has them, `AuthUser` second. The same shape as
[`../../src/projects/handlers.rs`](../../src/projects/handlers.rs):

```rust
pub async fn list_claims(
    State(state): State<AppState>,
    user: AuthUser,
    Query(list): Query<ClaimsListQuery>,          // parsed once, C-D0 I3
) -> AppResult<Json<ClaimPage>>

pub async fn create_claim(…, Json(payload): Json<CreateClaimRequest>)
    -> AppResult<(StatusCode, Json<ClaimDetailResponse>)>

pub async fn get_claim(…, Path(claim_id): Path<Uuid>) -> AppResult<Json<ClaimDetailResponse>>
pub async fn update_claim(…, Path(claim_id), Json(payload): Json<UpdateClaimRequest>) -> …
pub async fn delete_claim(…, Path(claim_id): Path<Uuid>) -> AppResult<StatusCode>
```

Each carries `#[utoipa::path(…, security(("bearer" = [])), …)]` with the
**absolute** path (amendment `A-C-D0-2`: `utoipa` cannot read `API_PREFIX`), and
the list handler's attribute names `params(ClaimQuery, PaginationQuery)` exactly
as the Phase 1 list handlers do (amendment `A-C-D0-7`). The OpenAPI document
gains the `claims` tag.

### 2.4 Components published by `R2`

`ClaimResponse`, `ClaimDetailResponse`, `ClaimPage`, `ItemResponse`,
`CreateClaimRequest`, `UpdateClaimRequest`, `ClaimStatus`. `Pagination` and the
error schemas already exist.

`ItemResponse` is published here, before the item endpoints exist, because the
published detail response **contains** items — `R2` cannot publish
`ClaimDetailResponse` without it. The warning in the brief ("no component that
`R3`/`R4` will need to change again") is respected: `R3` *adds*
`CreateItemRequest` and `UpdateItemRequest` and does not touch any of these
seven; `R4` adds no request body at all.

---

## 3. Layering, state and interfaces

```text
handlers.rs   ── HTTP only: extract, call one service method, pick the status
     │
service.rs    ── validate, resolve the project rule, map a repository miss to 404
     │
repository.rs ── the six statements; the only file that holds SQL
     │
models.rs     ── ExpenseClaim, ExpenseItem, ClaimStatus   (exists)
```

### 3.1 `ClaimService`

```rust
pub struct ClaimService {
    repository: ClaimRepository,
    messages: Arc<Messages>,
    pagination: PaginationSettings,
}

impl ClaimService {
    pub fn new(repository, messages, pagination) -> Self;

    pub async fn list(&self, owner: Uuid, query: ClaimQuery, pagination: PaginationQuery)
        -> AppResult<Page<ClaimResponse>>;
    pub async fn create(&self, owner: Uuid, request: CreateClaimRequest)
        -> AppResult<ClaimDetailResponse>;
    pub async fn get(&self, owner: Uuid, claim_id: Uuid)
        -> AppResult<ClaimDetailResponse>;
    pub async fn update(&self, owner: Uuid, claim_id: Uuid, request: UpdateClaimRequest)
        -> AppResult<ClaimDetailResponse>;
    pub async fn delete(&self, owner: Uuid, claim_id: Uuid) -> AppResult<()>;
}
```

`owner` is the first parameter of every method, as in `ProjectService`, so a
claim method cannot be called without saying whose claim is meant.
`AppState::new` builds it once: `ClaimService::new(ClaimRepository::new(db.clone()), messages.clone(), config.api.pagination)`,
and the field is `state.reimbursements`.

`list` returns `Page<ClaimResponse>`, not `Page<ClaimDetailResponse>`: the
listing does not carry items (§7).

### 3.2 `ClaimRepository`

Six methods, no trait. Five of them are one statement each; `list_owned` is a
page statement and a count statement in two shapes (§6.6), so the file holds
nine static statements and at most two run per request. The list input is a
struct rather than eight positional arguments, because
`clippy::too_many_arguments` fires at eight and an argument order that can be
silently swapped is a bug generator:

```rust
pub struct ClaimListFilter<'a> {
    pub status: Option<ClaimStatus>,
    pub currency: Option<&'a str>,        // already trimmed and upper-cased
    pub search: Option<&'a str>,          // already trimmed, non-empty
    pub submitted_from: Option<NaiveDate>,
    pub submitted_to: Option<NaiveDate>,
}

impl ClaimRepository {
    pub async fn insert(&self, id: Uuid, owner: Uuid, title: &str, description: &str,
                        currency: &str, project_id: Option<Uuid>)
        -> AppResult<Option<ExpenseClaim>>;          // None = project not the caller's
    pub async fn find_owned(&self, claim_id: Uuid, owner: Uuid)
        -> AppResult<Option<ExpenseClaim>>;
    pub async fn list_items_owned(&self, claim_id: Uuid, owner: Uuid)
        -> AppResult<Vec<ExpenseItem>>;              // read-only in R2
    pub async fn list_owned(&self, owner: Uuid, filter: &ClaimListFilter<'_>,
                            limit: i64, offset: i64)
        -> AppResult<(Vec<ExpenseClaim>, i64)>;
    pub async fn update_owned(&self, claim_id: Uuid, owner: Uuid,
                              title: Option<&str>, description: Option<&str>,
                              clear_project: bool, project_id: Option<Uuid>)
        -> AppResult<Option<ExpenseClaim>>;
    pub async fn delete_owned(&self, claim_id: Uuid, owner: Uuid) -> AppResult<bool>;
}
```

`list_items_owned` is not an item endpoint. It exists because the **published**
`GET /claims/{claim_id}` returns items; `R2` reads them and writes none.

---

## 4. The `Money` seam

### 4.1 What money is, in one paragraph

A money value is a JSON **string** with exactly four decimal places in both
directions. It is stored as `NUMERIC(19,4)`, selected as `::text`, carried in
Rust as `String`, and computed only by `SUM` in SQL. There is no `f32`, no
`f64`, no decimal crate and **no arithmetic in Rust** — this is
[`ADR-0007`](../decisions/ADR-0007-money-representation.md) and it is already
true of [`../../src/reimbursements/models.rs`](../../src/reimbursements/models.rs).

### 4.2 The response side (the whole of money in `R2`)

Every money field is `String` in the DTO and comes from a `::text` select:

```sql
SELECT …, total_amount::text AS total_amount, … FROM expense_claims
SELECT …, amount::text       AS amount,       … FROM expense_items
```

`NUMERIC(19,4)::text` renders four decimals (`"149.8000"`, `"0.0000"`), which
*is* the wire form; serde then quotes it as a JSON string. The explicit `AS`
alias is required: `RETURNING total_amount::text` is named after the column by
PostgreSQL today, but the alias makes the `FromRow` mapping independent of that
detail.

This is not a rule that can be violated silently: a money column selected
**without** `::text` does not decode (`sqlx` has no `NUMERIC` decoder with the
features in [`../../Cargo.toml`](../../Cargo.toml)), so the request fails loudly
at run time instead of emitting a wrong shape. The failure mode is "500 until
fixed", not "a number on the wire".

There is therefore **no money validation in `R2`**, because no `R2` request body
carries money: `POST` and `PATCH` never accept `total_amount` (§5.3), and the
item endpoints that accept `amount` are `R3`.

### 4.3 The input side (fixed here, built in `R3`)

The rule, and where it lives, are decided now so that `R3` does not decide them
per module.

**The rule.** A money string is accepted only if it matches
`^[0-9]{1,15}\.[0-9]{4}$`: at least one and at most fifteen ASCII digits, one
`.`, exactly four digits. No sign, no leading or trailing space, no grouping
separator, no exponent, no currency symbol, no empty string. `"0.0000"` and
`"999999999999999.9999"` are the bounds of the accepted range, which the digit
count already gives.

**Where it lives.** One function beside the other text validators:

```rust
// src/helpers/validation.rs (proposed for R3, not for R2)

/// Validates the textual form of a money value.
///
/// Returns the value unchanged: nothing parses it, nothing compares it and
/// nothing does arithmetic on it. The database sums money, in SQL.
pub fn validate_money(field: &str, value: &str, messages: &Messages) -> AppResult<String>;
```

It reports through `field_error`, reuses `AppError::Validation` and the
`validation_failed` code, and adds one catalogue string
(`messages.validation.money_scale`). It goes in `helpers` because it is exactly
what that module already holds — a character-level check that produces a
`FieldViolation` naming the field — and because `R3` (claim items) and `D3`
(job-application salary bounds) are two callers, which is the rule
[`../../src/helpers/mod.rs`](../../src/helpers/mod.rs) sets for a new helper.

**Why not a `Money` newtype** (the R2 plan in
[`../features/reimbursements/architecture.md`](../features/reimbursements/architecture.md) §2.3,
and the `src/money.rs` plan in [`module-boundaries.md`](module-boundaries.md)):

1. A newtype is a **second wire contract**. The field's JSON type is already
   fixed by `String` + the validator; a `Deserialize` impl that refuses a
   number produces *exactly* the same response as `String` does, because the
   JSON extractor in [`../../src/api/extract.rs`](../../src/api/extract.rs)
   reports a type mismatch as one violation for the field `body`
   ([`../api/error-contract.md`](../api/error-contract.md) §3). The newtype
   cannot name the field any better than the extractor already does, because
   the extractor only ever sees the whole-body error.
2. It would have no method but `new`. Every operation money needs — the sum,
   the range check, the comparison — is SQL's. A `Money` type with no
   arithmetic is a wrapper that exists to look like a numeral.
3. `R2` has no money input at all, so the type would have **zero callers** until
   `R3`, and [`../../src/helpers/mod.rs`](../../src/helpers/mod.rs) admits a
   helper only when more than one module uses it. A type that exists before the
   thing that needs it is a guess about the future, and this design does not
   need to guess: the rule is one function either way.

If a requirement ever appears that needs money handled *in Rust* (a pro-rated
split, a converted total), that is a new decision and a new ADR superseding
[`ADR-0007`](../decisions/ADR-0007-money-representation.md) — not an
implementation detail. Until then the rule is a text check in one place.

### 4.4 Enforcing four decimals on input, and why the database cannot do it

**The column is permissive, so the service must not be.** PostgreSQL coerces a
value to the declared scale instead of refusing it:

| Input | `NUMERIC(19,4)` stores | Would a client notice? |
| ----- | ---------------------- | ---------------------- |
| `"12.5000"` | `12.5000` | no — it is the contract |
| `"12.5"` | `12.5000` | **no** — silently accepted, silently re-rendered as `"12.5000"` |
| `"12.500"` | `12.5000` | **no** — same |
| `"12.50001"` | `12.5000` | **no** — silently **rounded** |
| `"12.50005"` | `12.5001` | **no** — rounded *up* |

[`../../tests/claims_schema.rs`](../../tests/claims_schema.rs)
(`money_is_stored_with_four_decimal_places`) records this behaviour, and the
`C-R1` checkpoint states it in the same words: the four-decimal wire form is a
property of the column's *output*, not of its *input*.

So the rule is enforced **before** the statement runs, by `validate_money`, and
`"12.5"` and `"12.500"` are a `422` — not a silent `12.5000`. Two reasons:

1. **The published contract says so.** `conventions.md` §15 lists `"125.5"` and
   `"125.500"` under *Rejected forms*, and says under *Rounding*: "**None.** The
   server never rounds a money value; it refuses one that is not already at the
   scale." Accepting `"12.5"` would make that sentence false.
2. **The round trip would stop being identity.** `ADR-0007` promises that the
   string a client sends is the string it reads back. With silent coercion,
   `"12.5"` in becomes `"12.5000"` out, and `"12.50001"` in becomes `"12.5000"`
   out — a client comparing its input with a later read would be told it sent
   something it did not.

The database remains a backstop for the *range* (`expense_items_amount_chk` →
`23514`, `expense_claims_total_amount_chk`) and the *type*. A malformed string
(`"abc"`) bound to a `numeric` parameter is SQLSTATE `22P02`; if one ever gets
past the validator it is a `500`, not a `422`, because a value reaching that
point is a bug in this layer, not a client error.

**One case the validator accepts and the column canonicalises:** `"007.0000"` is
within `^[0-9]{1,15}\.[0-9]{4}$` and within range, and it reads back as
`"7.0000"`. The published contract does not list a leading zero as a rejected
form, so refusing it would be an invented `422`; the round trip is then
*value*-exact, not byte-exact, for a non-canonical but valid input. This is
recorded as a limitation (§10), not fixed silently.

---

## 5. PATCH semantics

`AGENTS.md` §4.6: "An omitted field is left untouched. Do not write `null` over
an existing value." [`../api/conventions.md`](../api/conventions.md) §7: the
same rule, plus "a body with no field at all is accepted and changes nothing
except `updated_at`".

### 5.1 The three patchable fields

| Field | Absent | Present | Validation |
| ----- | ------ | ------- | ---------- |
| `title` | untouched | written | `required_text("title", …, MAX_TITLE_LENGTH)`: trimmed, not empty, ≤ 200 |
| `description` | untouched | written | `bounded_text("description", …, MAX_DESCRIPTION_LENGTH)`: kept verbatim, ≤ 5000; `""` clears it |
| `project_id` | untouched | written or cleared | a UUID must belong to the caller (`404` otherwise); `null` clears the link |

`{"description": null}` deserialises into `Option<String>` as `None`, which is
indistinguishable from an absent field, so it is treated as **omitted** — it
does not clear anything. The database column is `NOT NULL DEFAULT ''`, the
published type is `string` (not "string or null"), and "cleared" has exactly one
spelling in the contract: `""`.

`{"title": null}` behaves the same way and is ignored. `{"title": ""}` (or a
title of spaces) is **not** a clear — a claim must always have a title, so it is
a `422` naming `title`, exactly as on create.

`{"description": ""}` is the one empty string that means something: it stores
`''`, which is the column's own default and a legal value under
`expense_claims_description_length_chk`.

### 5.2 `project_id` is a tri-state, and two things stand in the way

`Option<Option<Uuid>>` is the smallest faithful *shape*, but it is not faithful
on its own. Two pieces of machinery are needed, and both must be written down or
the tri-state silently collapses to a pair.

**`serde` collapses `null` into "absent" by default.** `Deserialize for
Option<T>` calls `deserialize_option`, and `serde_json` answers an explicit
`null` with `visit_none` — for the *outer* `Option` — so a plain
`Option<Option<Uuid>>` field yields `None` for both `{}` and
`{"project_id": null}`. The field therefore carries **both** attributes, and
each has a job:

```rust
// src/reimbursements/dto.rs (proposed)

#[serde(default, deserialize_with = "double_option")]
pub project_id: Option<Option<Uuid>>,

/// Distinguishes `"project_id": null` from an absent key.
///
/// `#[serde(default)]` handles absence (`None`) and short-circuits this
/// function; when the key *is* present this function runs, and `null`
/// deserialises as `Option::<Uuid>::deserialize(..) == None`, wrapped into
/// `Some(None)`. Drop `default` and the missing-field deserializer answers
/// `visit_none`, which puts absence back to `Some(None)`; drop this function
/// and `null` becomes `None`. Both are required.
fn double_option<'de, D>(deserializer: D) -> Result<Option<Option<Uuid>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<Uuid>::deserialize(deserializer).map(Some)
}
```

No new crate: this is eight lines of `serde`, not a `serde_with` dependency
(`AGENTS.md` §4.8).

**The three body forms, and what the statement does with each:**

| Body | `request.project_id` | `clear_project` | `project_id` bound | Statement effect |
| ---- | -------------------- | --------------- | ------------------ | ---------------- |
| key absent | `None` | `false` | `NULL` | `COALESCE(NULL, project_id)` — untouched |
| `"project_id": null` | `Some(None)` | `true` | `NULL` | `CASE WHEN $5 THEN NULL` — cleared |
| `"project_id": "<uuid>"` | `Some(Some(id))` | `false` | `Some(id)` | set, after the ownership predicate |

The mapping is total and lives in one place, an accessor on the request:

```rust
impl UpdateClaimRequest {
    /// `(clear, value)` — the two parameters the statement binds.
    pub fn project_change(&self) -> (bool, Option<Uuid>) {
        match self.project_id {
            None => (false, None),
            Some(None) => (true, None),
            Some(Some(id)) => (false, Some(id)),
        }
    }
}
```

A single `COALESCE($n, project_id)` cannot express "set to NULL" — `NULL`
already means "leave alone" — which is why the boolean exists. A unit test
deserializes all three bodies through `serde_json::from_str` and then asserts
both halves, so neither the attributes nor the pair can drift.

### 5.3 What a PATCH body must **not** be able to change

`currency`, `status`, `total_amount`, `id`, `user_id`, `created_at` and the three
lifecycle timestamps are not fields of `UpdateClaimRequest`. serde ignores
unknown keys by default, so a body carrying them is accepted (`200`) and they
are **ignored**. In particular:

- **`currency` is immutable because it is unwritable, not because it is
  checked.** No `422` is produced for `{"currency": "USD"}` — see §9.1, where
  this contradicts four documents and the published contract wins.
- **`status` is unchanged by any `PATCH`.** The only writer of `status` will be
  the four action endpoints in `R4`.
- **`#[serde(deny_unknown_fields)]` must not be added.** It would turn every
  unknown key into a `422` — a breaking change for any client that sends one
  extra field — and it would contradict §5.3's own rule that extra keys are
  ignored.

The SQL enforces the same thing structurally: the `SET` clause of `update_owned`
names `title`, `description`, `project_id` and `updated_at`, and nothing else.
There is no code path in which a handler could pass a currency through.

### 5.4 An empty PATCH body

`{}` is a valid body: every field is `Option`, so the statement runs with all
three "untouched" values, and the only change is `updated_at = NOW()`. The
response is `200` with the claim unchanged apart from `updated_at`. This is the
conventions §7 rule, and it is asserted rather than assumed.

---

## 6. The repository statements

Every statement carries the caller's `user_id` in its own `WHERE`, not in a
check ([`ADR-0005`](../decisions/ADR-0005-ownership-enforced-in-sql.md)). A miss
is a `404` in `ClaimService`, with **the same** message for "does not exist" and
"belongs to somebody else".

### 6.1 `insert` — create, with the project rule in the statement

```sql
INSERT INTO expense_claims (id, user_id, title, description, currency, status, project_id)
SELECT $1, $2, $3, $4, $5::char(3), 'draft'::expense_claim_status, $6::uuid
WHERE $6::uuid IS NULL
   OR EXISTS (SELECT 1 FROM projects WHERE id = $6::uuid AND user_id = $2)
RETURNING id, user_id, title, description, currency,
          total_amount::text AS total_amount, status, project_id,
          submitted_at, decided_at, paid_at, created_at, updated_at
```

Binds: `id`, `owner`, `title`, `description`, `currency`, `project_id`.

- **Ownership predicate:** `$2` (the caller) appears in the `EXISTS` over
  `projects`. `user_id` is written from the caller and never from the request.
- **The `404` for a foreign project, with no second round trip:** the
  `SELECT … WHERE` either produces one row or none. `None` therefore means
  exactly "the project does not exist or is not the caller's" — the id is a
  fresh UUID, `owner` comes from a valid token, and `title`/`description`/
  `currency` were validated before the statement, so nothing else can suppress
  the row (a primary-key collision raises `23505`, which is a `500`, not
  `None`). The service maps `None` to `404 not_found` with
  `messages.resources.project_not_found`.
- **The check and the write are one statement**, which is what makes "the
  project was deleted between the check and the insert" impossible: there is no
  gap. This is the `ADR-0005` shape used by task creation.
- `'draft'` is written **explicitly** even though it equals the column default,
  because "a claim is created in `draft`" is a promise in
  [`../features/reimbursements/api.md`](../features/reimbursements/api.md) §4;
  a statement that relies on a default makes that promise a migration detail.
  `total_amount` comes from the default `0`, which renders `"0.0000"`.
  `submitted_at`, `decided_at` and `paid_at` stay `NULL`, which is what
  `expense_claims_lifecycle_timestamps_chk` requires for `draft`.

### 6.2 `find_owned` — the read, and the fallback that names the right resource

```sql
SELECT id, user_id, title, description, currency,
       total_amount::text AS total_amount, status, project_id,
       submitted_at, decided_at, paid_at, created_at, updated_at
FROM expense_claims
WHERE id = $1 AND user_id = $2
```

**Ownership predicate:** `id = $1 AND user_id = $2`. Used by `get`, and by
`update` on the failure path (§6.4).

### 6.3 `list_items_owned` — the detail read

```sql
SELECT id, user_id, claim_id, description,
       amount::text AS amount, incurred_on, category, receipt_url,
       created_at, updated_at
FROM expense_items
WHERE claim_id = $1 AND user_id = $2
ORDER BY incurred_on, created_at, id
```

**Ownership predicate:** `claim_id = $1 AND user_id = $2`. Both, always — an
item id is never enough on its own, and here even the claim id is paired with
the owner so the statement cannot return another account's rows if the claim
lookup above ever changed. The order is the published one
([`../features/reimbursements/api.md`](../features/reimbursements/api.md) §2.3
and [`../database/schema.md`](../database/schema.md) §11.2); it is served by
`expense_items_claim_id_idx` on the `claim_id` lookup, with the sort over one
claim's rows.

### 6.4 `update_owned` — the partial update, with the project rule

```sql
UPDATE expense_claims
SET title       = COALESCE($3, title),
    description = COALESCE($4, description),
    project_id  = CASE WHEN $5 THEN NULL ELSE COALESCE($6::uuid, project_id) END,
    updated_at  = NOW()
WHERE id = $1
  AND user_id = $2
  AND ($6::uuid IS NULL
       OR EXISTS (SELECT 1 FROM projects WHERE id = $6::uuid AND user_id = $2))
RETURNING id, user_id, title, description, currency,
          total_amount::text AS total_amount, status, project_id,
          submitted_at, decided_at, paid_at, created_at, updated_at
```

Binds: `claim_id`, `owner`, `title`, `description`, `clear_project`, `project_id`
— exactly the six of §5.2.

- **Ownership predicates, both of them:** `id = $1 AND user_id = $2` for the
  claim, and `user_id = $2` inside the `EXISTS` over `projects` for the link.
- **`None` is ambiguous, and the service resolves it once, on the error path:**
  zero rows means *either* the claim is absent/foreign *or* the project is. The
  service then runs **one** `find_owned(claim_id, owner)`:
  - it returns a row → the claim exists, so the project was the problem →
    `404 not_found` with `messages.resources.project_not_found`;
  - it returns nothing → `404 not_found` with `messages.resources.claim_not_found`.

  This is not a check-then-act. The authoritative decision — the write — is one
  statement; the extra read happens only after that statement has already
  refused, it is owner-scoped, and it changes no state. The alternative (read
  the project *first*, then update) was rejected because it puts a round trip
  and a race on the **success** path, which is every request. For `create` no
  disambiguation is needed at all, because there the claim is new (§6.1).
- **`currency`, `status` and the timestamps are not in the `SET` clause**, so
  they cannot be written by any `PATCH` (§5.3).
- **`COALESCE($n, column)`** is the conventions §7 rule made mechanical: a
  handler that forgot a field cannot null it, because `NULL` in the binds means
  "keep".

### 6.5 `delete_owned` — and the items

```sql
DELETE FROM expense_claims WHERE id = $1 AND user_id = $2
```

**Ownership predicate:** `id = $1 AND user_id = $2`. The result is
`rows_affected() > 0`; `false` is `404`. The items go in the same statement,
through `expense_items_claim_fk … ON DELETE CASCADE`, which
`deleting_a_claim_deletes_its_items` already proves. No transaction is needed
and none is used, because there is exactly one statement.

### 6.6 `list_owned` — two shapes, one per index

`schema.md` §11.1 gives two listing indexes and says why:
`expense_claims_user_id_created_at_idx` for the unfiltered list and
`expense_claims_user_id_status_created_at_idx` for the list filtered by status.
A single statement that wrote `($n IS NULL OR status = $n)` would **hide the
status from the planner** and leave the second index unread, so the repository
holds two shapes, exactly as `ProjectRepository` holds a page and a count for
each of its two search cases.

**Shape A — no status filter** (`$1 owner`, `$2 currency`, `$3 from`, `$4 to`,
`$5 search`, `$6 limit`, `$7 offset`):

```sql
SELECT id, user_id, title, description, currency,
       total_amount::text AS total_amount, status, project_id,
       submitted_at, decided_at, paid_at, created_at, updated_at
FROM expense_claims
WHERE user_id = $1
  AND ($2::char(3) IS NULL OR currency = $2::char(3))
  AND ($3::date IS NULL OR submitted_at >= ($3::date::timestamp AT TIME ZONE 'UTC'))
  AND ($4::date IS NULL OR submitted_at <  (($4::date + 1)::timestamp AT TIME ZONE 'UTC'))
  AND ($5::text IS NULL
       OR title ILIKE '%' || $5 || '%'
       OR description ILIKE '%' || $5 || '%')
ORDER BY created_at DESC, id DESC
LIMIT $6 OFFSET $7
```

**Shape B — with the status filter** — the same `SELECT`, `FROM` and `WHERE
user_id = $1` as Shape A, then `$2` is the status and the remaining parameters
shift by one:

```sql
  AND status = $2::expense_claim_status
  AND ($3::char(3) IS NULL OR currency = $3::char(3))
  AND ($4::date IS NULL OR submitted_at >= ($4::date::timestamp AT TIME ZONE 'UTC'))
  AND ($5::date IS NULL OR submitted_at <  (($5::date + 1)::timestamp AT TIME ZONE 'UTC'))
  AND ($6::text IS NULL
       OR title ILIKE '%' || $6 || '%'
       OR description ILIKE '%' || $6 || '%')
ORDER BY created_at DESC, id DESC
LIMIT $7 OFFSET $8
```

And two matching counts, `SELECT COUNT(*)` with the same `WHERE` and no
`ORDER`/`LIMIT`, so `pagination.total` is the size of the set the page was taken
from — never the whole collection.

Four notes on the SQL:

1. **Every optional parameter carries an explicit cast** (`::char(3)`, `::date`,
   `::text`, `::expense_claim_status`). A parameter that appears only as
   `$n IS NULL` has no expression from which PostgreSQL can infer its type, so
   the cast is what makes the statement's parameter types definite; it is also
   what makes the comparison against `currency CHAR(3)` and `status
   expense_claim_status` explicit rather than left to operator resolution. This
   is the `($n IS NULL OR …)` form `schema.md` §11.1 describes for these
   filters.
2. **The date bounds are UTC days, half-open at the top.** `>= from 00:00:00Z`
   and `< (to + 1) day 00:00:00Z` is "inclusive of both ends" without
   `<= to 23:59:59.999999`, and it does not depend on the session `TimeZone`,
   which a bare `date::timestamptz` would.
3. **A draft is excluded when either bound is present, by SQL's own semantics.**
   `submitted_at >= …` and `submitted_at < …` are `NULL` for a NULL
   `submitted_at`, and `NULL` is not true, so the row drops out. When both
   bounds are absent, the first disjunct of each line is true and drafts are
   included.
4. **The search term is used unescaped**, exactly as `ProjectRepository` and
   `NoteRepository` use it: a `%` or `_` in the term is a wildcard. That is
   existing API behaviour, and changing it here would make two searches behave
   differently; it is recorded in §10 rather than fixed in one feature.

### 6.7 What the two reads of the detail do and do not guarantee

`get` and `update` read the claim and then its items, in two statements and
therefore two `READ COMMITTED` snapshots. In `R2` nothing can race them — there
is no item writer. **`R3` introduces one**, and it must decide whether the detail
read is wrapped in a single `REPEATABLE READ` transaction (so `total_amount` and
the items always agree) or whether a total that lags the items by one
concurrent write is acceptable. That decision is `R3`'s, it changes no response
shape, and it is named here so that `R2` is not accused of having settled it by
accident.

---

## 7. Listing: pagination, filters, order

### 7.1 The query type, parsed once

One extractor per list handler (amendment `A-C-D0-6`/`I3`), one type:

```rust
// published, and named in the #[utoipa::path(params(...))] attribute
pub struct ClaimQuery {
    pub status: Option<ClaimStatus>,
    pub currency: Option<String>,
    pub search: Option<String>,
    pub submitted_from: Option<NaiveDate>,
    pub submitted_to: Option<NaiveDate>,
}

// deserialized only; absent from the OpenAPI document on purpose
pub struct ClaimsListQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<ClaimStatus>,
    pub currency: Option<String>,
    pub search: Option<String>,
    pub submitted_from: Option<NaiveDate>,
    pub submitted_to: Option<NaiveDate>,
}

impl ClaimsListQuery { pub fn into_parts(self) -> (ClaimQuery, PaginationQuery) }
```

The fields are written out, not `#[serde(flatten)]`ed, for the reason recorded
in [`../api/conventions.md`](../api/conventions.md) §5: `serde_urlencoded`'s map
access yields strings, so a flattened `Option<i64>` fails at run time.
`ClaimsListQuery` derives `Deserialize` only — no `ToSchema`, no
`IntoParams` — so the published parameter set stays exactly `ClaimQuery` +
`PaginationQuery`.

### 7.2 The published parameter set

`limit`, `offset`, `status`, `currency`, `search`, `submitted_from`,
`submitted_to` — each exactly once. This is the set
[`../features/reimbursements/api.md`](../features/reimbursements/api.md) §2.6
and §15 publish, and the set
`each_list_endpoint_publishes_its_filter_and_pagination_parameters_once` must
gain for `GET /api/v1/claims`.

| Parameter | Type | Default | Rule | Where enforced |
| --------- | ---- | ------- | ---- | -------------- |
| `limit` | integer | `20` | above `max_limit` clamped; `≤ 0` → default | `PaginationQuery::effective_limit` |
| `offset` | integer | `0` | negative → `0` | `PaginationQuery::effective_offset` |
| `status` | enum | — | one of the five; anything else `400` | the query deserializer |
| `currency` | string | — | trimmed, upper-cased, exactly three ASCII letters; anything else `422` | `ClaimService::list` |
| `search` | string | — | trimmed, case-insensitive substring over `title` and `description`, ≤ 200; empty = no filter | `validate_search` (reused) |
| `submitted_from` | date | — | `YYYY-MM-DD`, UTC, inclusive lower bound | the query deserializer (`400`) + the statement |
| `submitted_to` | date | — | `YYYY-MM-DD`, UTC, inclusive upper bound | the same |
| `submitted_from` + `submitted_to` | date pair | — | `submitted_from` later than `submitted_to` is a `422` naming `submitted_from` | `ClaimService::list`, before the statement |

The defaults and the clamp are the configured values
(`api.pagination.default_limit` / `max_limit`), not constants — changing either
is a breaking change for a client, per conventions §5.

**Where each failure lands, and why** (the `400`/`422` split is the one clients
get wrong, so it is stated explicitly):

| Input | Status | Reason |
| ----- | ------ | ------ |
| `?limit=abc`, `?offset=1.5` | `400` | the query string cannot be parsed into an `i64` |
| `?submitted_from=not-a-date`, `?submitted_from=2026-02-30`, `?submitted_from=` | `400` | the query string cannot be parsed into a date |
| `?status=archived`, `?status=Submitted`, `?status=` | `400` | outside the enum, so it never became a `ClaimStatus` |
| `?currency=EU`, `?currency=EURO`, `?currency=eur1`, `?currency=` | `422` | it parsed as a string and broke the three-letter rule |
| `?search=` | no filter | the published "empty means no filter" |
| `?search=<201 chars>` | `422` naming `search` | the shared validator |
| `?submitted_from=2026-10-31&submitted_to=2026-10-01` | `422` naming `submitted_from` | the order rule |

`?currency=` is a `422` and not "no filter": the published rule is "three ASCII
letters, otherwise `422`" with no exception, and `search` is the only parameter
for which an empty value is documented to mean "no filter". Following the
contract literally is preferable to inventing a second exception. If the owner
wants `currency=` to clear the filter, that is a change to
[`../features/reimbursements/api.md`](../features/reimbursements/api.md), not an
implementation choice.

### 7.3 Order and the index that serves it

**Order: `created_at DESC, id DESC`. There is no client-selectable sort**
(published; conventions §16.3). The identifier is the tie-breaker, so the order
is total and two pages cannot overlap or skip a row.

| Request | Index | Why |
| ------- | ----- | --- |
| no `status` | `expense_claims_user_id_created_at_idx (user_id, created_at DESC)` | equality on the leading column, order matching the index direction |
| with `status` | `expense_claims_user_id_status_created_at_idx (user_id, status, created_at DESC)` | two equalities, then the order column |

`id DESC` is applied within each `created_at` group — an incremental sort over
the equal-timestamp rows the index produced, not a full sort. `currency`, the
date range and the search are **not** indexed and must not be: they are applied
to the rows the chosen index has already narrowed to the owner, which is what
[`../database/schema.md`](../database/schema.md) §11.1 says and why no
`currency` index exists.

`expense_claims_project_id_idx` is **not** used by `R2`: the published listing
has no `project_id` filter. The index serves the `ON DELETE SET NULL`
maintenance and a filter the contract does not offer.

`pagination.total` comes from the matching count statement, so it counts the
filtered set; `has_more` is `offset + limit < total`
([`../../src/pagination.rs`](../../src/pagination.rs)). An offset past the end
is an empty page, not a `404`.

---

## 8. Where the errors come from

| Situation | Variant | Status | Code |
| --------- | ------- | ------ | ---- |
| body is not JSON, path parameter is not a UUID, a query parameter cannot be parsed | `BadRequest` from the extractor | `400` | `bad_request` |
| a query value outside its enum (`?status=archived`) | the query extractor's own rejection | `400` | `bad_request` |
| a body value of the wrong type, a missing required body field (`project_id: "abc"`, no `title`) | the JSON extractor's rejection | `422` | `validation_failed` |
| `title`, `description`, `currency`, `search`, the date range | `AppError::validation` via `field_error` | `422` | `validation_failed` |
| no token, a bad token, a token for a deleted account | `Unauthorized` from `AuthUser` | `401` | `unauthorized` |
| the claim is absent or another owner's; the project is absent or another owner's | `AppError::NotFound` in the service | `404` | `not_found` |
| body over `max_body_bytes` | `PayloadTooLarge` in the extractor | `413` | `payload_too_large` |
| the request exceeded the timeout | `Timeout` in the middleware | `408` | `timeout` |
| a statement failed | `AppError::Database` | `500` | `database_error` |

Two rules the service follows, in this order:

1. **Validate the whole body first, then touch the database.** A request that is
   both malformed and aimed at a foreign resource is a `422`, not a `404`: the
   body never had to be read.
2. **Every repository miss becomes `NotFound` with a catalogue message.** The
   service never compares identifiers and never returns `403` for a resource
   (`ADR-0005` §3; error-contract §6).

The three catalogue entries `R2` needs (added to
[`../../config/messages.toml`](../../config/messages.toml) **and** to
[`../../src/config/messages.rs`](../../src/config/messages.rs) in the same
change):

| Key | Text | Used by |
| --- | ---- | ------- |
| `messages.resources.claim_not_found` | `The requested expense claim does not exist.` | `GET`, `PATCH`, `DELETE` miss |
| `messages.validation.currency_invalid` | `must be exactly three letters` | `create`, and the `currency` filter |
| `messages.validation.date_range_order` | `must not be later than submitted_to` | the list, naming `submitted_from` |

These are **messages, not settings**: no environment variable, no TOML setting,
no change to the reference tables in
[`deployment.md`](deployment.md) or [`../../config/README.md`](../../config/README.md).

---

## 9. Where the documents disagree

The brief asks for every disagreement between the published contract and the
schema, resolved explicitly rather than silently. These are the real ones found
by reading `api.md`, `schema.md` §11, the `C-R1` contract and the migration.

### 9.1 `currency` on `PATCH`: ignored (contract) versus `422` (schema and plan)

**The disagreement.**

| Document | Says |
| -------- | ---- |
| [`../features/reimbursements/api.md`](../features/reimbursements/api.md) §6 | `currency`, `status` and the timestamps "are **not** updatable. They are absent from the request contract … a client that sends them does not change them." No `422` is listed for it. |
| [`../features/reimbursements/testing.md`](../features/reimbursements/testing.md) §2.1 | `a_claim_can_be_partially_updated` sends `{"currency":"USD","status":"paid"}`, expects the currency and status unchanged — i.e. `200`. |
| [`../database/schema.md`](../database/schema.md) §11.1 | "`currency` is immutable (`REIMB-R02`). That is a service rule with a `422` and a test, not a trigger." |
| [`../features/reimbursements/status.md`](../features/reimbursements/status.md) §1 | "Immutability is a service rule with a `422`". |
| [`C-R1`](../engineering/runs/archive/phase-2/ledger.md) `I6` | "`currency` is immutable (service rule with `422`, from R2)". |

**The resolution: `api.md` wins, and the others are corrected.** A `PATCH`
carrying `currency` (or `status`, or `total_amount`) is accepted with `200` and
the field is ignored. Reasons, in order of weight:

1. `api.md` is the published client contract; the others are internal documents
   about it. `AGENTS.md` §4.10 makes a contract change explicit, and this design
   is not the place to change one.
2. Producing a `422` *only* for `currency` requires **adding** `currency` and
   `status` to `UpdateClaimRequest` in order to reject them. That widens the
   published request schema with two fields nothing uses, and contradicts
   "they are absent from the request contract" in the same paragraph.
3. `#[serde(deny_unknown_fields)]` is not an alternative: it would make **every**
   unknown key a `422`, which is a breaking change for any client that sends one
   extra field.

The immutability guarantee is unchanged and is arguably stronger: the column is
not in the `SET` clause of any statement, so `currency` has no write path at
all. What is dropped is the *error*, not the rule.

**Owners to correct, in the `R2` change, by the agents that own them:** the
`database` agent (`schema.md` §11.1), the `docs` agent (`status.md` §1 and the
`R02` row, and
[`../features/reimbursements/architecture.md`](../features/reimbursements/architecture.md)
invariant 4), the orchestrator (the `C-R1` `I6` line in the ledger — recorded
here rather than edited, because the ledger is the run record).

### 9.2 A four-character currency: type error before the `CHECK`

`schema.md` §11.1 says `expense_claims_currency_chk` (`currency ~ '^[A-Z]{3}$'`)
enforces the currency. The `C-R1` checkpoint records what the running database
actually does: `CHAR(3)` refuses a four-character value with SQLSTATE `22001`
before the `CHECK` is evaluated. **The intent holds — such a value is
unrepresentable — but the named mechanism is wrong.** This changes nothing for
`R2`: the service rejects a non-three-letter currency with a `422` before any
statement runs, so neither the type error nor the `CHECK` is reachable from the
API. It is repeated here only because the brief asks for the disagreement to be
stated, and because a reviewer will read `schema.md` §11.1 and see the wrong
name.

### 9.3 `search` and the `currency` index, as text that `R2` makes stale

`schema.md` §11.1 says "A separate index on `currency` would be written on every
insert and **read by no statement**". With `R2`, the listing statement *does*
read the `currency` column — as an optional predicate applied to rows the
listing index already produced. The sentence is right in the sense that matters
(no index *seek* on currency, so no index is worth building) and wrong if read
literally. A one-word correction ("no statement *seeks* by `currency`") is
enough; it is the `database` agent's file.

### 9.4 Two smaller notes, not disagreements

- **Timestamp rendering.** `api.md` and
  [`../api/conventions.md`](../api/conventions.md) §6 show
  `2026-10-21T09:15:00.000000Z`. `chrono`'s serde implementation emits
  `SecondsFormat::AutoSi`: six fractional digits when the value has sub-second
  precision (which `NOW()` almost always gives), and **no** fractional part when
  it does not. Both are valid RFC 3339 UTC, which is the contract. **The tests
  must assert the instant, not the literal string** — compare a parsed
  `DateTime<Utc>`, or assert only that the field is present and non-null.
- **`DELETE` in any status.** `api.md` §7 and `REIMB-R11` allow it; the schema
  has no status condition on the row. No conflict; `R2` simply does not guard
  the delete by status.

---

## 10. Known limitations this design accepts

| Limitation | Why it is accepted |
| ---------- | ------------------ |
| A `PATCH` with a foreign `project_id` costs one extra owner-scoped read **on the failure path** to name the resource correctly. | The success path is one statement. Reading the project first would put a round trip and a check-then-act race on every request. |
| `"007.0000"` is accepted and reads back `"7.0000"`. | The published contract does not refuse a leading zero, and inventing a `422` would be a contract change dressed as validation. The round trip is value-exact. |
| A `%` or `_` in `search` is a wildcard. | Identical to `notes` and `projects` today. Fixing it in one feature would make two searches behave differently; it is a cross-feature change. |
| `?currency=` is a `422`, while `?search=` is "no filter". | Each follows the published sentence for that parameter; see §7.2. |
| The detail read is two statements in two snapshots. | Nothing writes items in `R2`, so nothing can race it. `R3` owns the decision (§6.7). |
| Deleting a claim is permanent, including in `approved` and `paid`. | `REIMB-R11`, an accepted requirement; `specification.md` §9 records the product view. |
| `R2` cannot prove that `total_amount` equals the sum of the items, because only `R3` writes items. | The database half is proved by `tests/claims_schema.rs` and the seeder; the API half is `R3`'s. `R2` must not claim `REIMB-R03`. |

---

## 11. What `R2` must not do

1. **No item endpoint.** `POST`/`PATCH`/`DELETE …/items` are `R3`. `R2` reads
   items (the detail response needs them) and writes none.
2. **No lifecycle action.** No `/submit`, `/approve`, `/reject`, `/pay`, no
   `ensure_transition`, no `409`, no writer of `status`, `submitted_at`,
   `decided_at` or `paid_at`.
3. **No component that `R3`/`R4` will have to change.** The seven published
   components are final for this feature (§2.4). `R3` adds two request schemas;
   `R4` adds none.
4. **No money type and no money validator.** No `Money` newtype in `dto.rs`, no
   `src/money.rs`, no `validate_money` with zero callers (§4.3). No `f32`/`f64`
   anywhere on a money path.
5. **No new dependency.** No decimal crate, no `sqlx` `json` feature (which
   `json_agg` would require for a single-statement detail read), no date crate.
   `NaiveDate` and `Uuid` are already used.
6. **No new setting.** No environment variable and no TOML setting. Three
   catalogue strings, which are messages, not settings (§8).
7. **No migration.** `0004` is applied; nothing in this chain changes a column,
   a constraint or an index.
8. **No transaction in `R2`.** The only candidate is the two-statement detail
   read, and §6.7 assigns that decision to `R3`.
9. **No `403`.** A foreign resource is `404`. The only `403` in the API remains
   the logout-of-another-session case.
10. **No change to the Phase 1 modules.** `notes`, `projects`, `tasks` are read
    only through their own tables; `reimbursements` calls no other feature's
    service.
11. **No `unwrap`/`expect` in the request path**, no `allow`, no `ignore`, no
    `TODO`. `Uuid::new_v4()` is called in the service, as in `ProjectService`.
12. **No existing test deleted, renamed or ignored.** The changes to
    `tests/openapi_contract.rs` extend an existing expectation and add three new
    cases; the counts in `implementation-status.md` and the ledger move by the
    number of tests actually added, and by nothing else.

---

## 12. Acceptance tests

The names below are what `C-R2` fixes. They are written in the style of the
`C-R1` test list and land in `tests/claims_api.rs` (new), in
`tests/openapi_contract.rs` (extended) and as unit tests inside
`src/reimbursements/`. Names already planned in
[`../features/reimbursements/testing.md`](../features/reimbursements/testing.md)
§2 are reused verbatim so that the feature document and the suite keep pointing
at the same rows; where `R2` needs a rule that document does not name, the new
name is marked **(new)**.

### 12.1 `tests/claims_api.rs` — a claim can be created, read, changed and deleted

- `a_claim_can_be_created_read_and_deleted`
- `a_claim_can_be_created_with_a_project_of_the_caller`
- `a_claim_can_be_partially_updated`
- `a_patch_body_with_no_fields_changes_only_the_modification_time` **(new)**
- `the_claim_list_returns_the_callers_claims_newest_first` **(new)**

### 12.2 `tests/claims_api.rs` — ownership: the second account gets `404`

- `a_claim_of_another_user_is_invisible`
- `the_listing_only_contains_the_callers_claims`
- `a_claim_cannot_reference_another_users_project`
- `deleting_the_project_clears_the_link_and_keeps_the_claim`

### 12.3 `tests/claims_api.rs` — money on the wire

`R2` publishes money and accepts none: `total_amount` is server-owned, and the
item endpoints that accept `amount` are `R3`. The request-half tests in
[`../features/reimbursements/testing.md`](../features/reimbursements/testing.md)
§2.2 — `a_json_number_where_money_is_expected_is_refused`,
`an_amount_with_the_wrong_number_of_decimals_is_refused`,
`a_negative_amount_is_refused`, `an_amount_above_the_maximum_is_refused` — are
`R3`'s and must not be claimed by this chain.

- `money_round_trips_with_four_decimals` (the `R2` half: `"0.0000"` on create, a fixture total of `"149.8000"` read back unchanged)
- `a_money_field_is_never_a_json_number_in_a_response` **(new)** — `total_amount` and every item `amount` parse as a JSON string with exactly four decimals, asserted with a type check rather than by comparison
- `the_total_is_not_accepted_from_the_client`

### 12.4 `tests/claims_api.rs` — `422` validation

`testing.md` §2.5 plans one case named
`the_claim_currency_is_required_and_immutable`. `R2` splits it in two, because
the create rule and the patch rule are different failures and only the second
one is the §9.1 resolution: `the_claim_currency_is_required_and_upper_cased`
(create) and `the_claim_currency_is_not_patchable` (patch). The feature document
should be split in the same change rather than keep a name that no longer
matches a test.

- `a_claim_title_must_not_be_blank`
- `a_claim_title_is_bounded`
- `a_claim_title_is_trimmed_before_it_is_measured` **(new)**
- `a_claim_description_is_bounded`
- `the_claim_currency_is_required_and_upper_cased` **(new)** — missing → `422`; `"EU"` → `422`; `"eur"` stored and returned as `"EUR"`
- `the_claim_currency_is_not_patchable` **(new)** — `PATCH {"currency":"USD"}` is `200` and the stored currency is unchanged; this is the §9.1 resolution, asserted
- `the_claim_status_is_not_patchable` **(new)** — `PATCH {"status":"paid"}` is `200` and the claim is still `draft`
- `a_project_id_that_is_not_a_uuid_is_refused` **(new)** — `422`, field `body`
- `a_reversed_date_range_is_refused`

### 12.5 `tests/claims_api.rs` — `400` malformed and `413` over-size

- `a_claim_id_must_be_a_uuid`
- `a_malformed_body_is_refused`
- `an_unknown_status_filter_is_refused`
- `a_malformed_claim_list_parameter_is_a_bad_request` **(new)** — `limit=abc` and `submitted_from=not-a-date`, one case each
- `a_claims_body_over_the_limit_is_refused` (`413 payload_too_large`)

### 12.6 `tests/claims_api.rs` — listing, filters, pagination

- `the_claims_listing_filters_by_status_and_currency`
- `the_claims_listing_filters_by_submission_dates`
- `the_claim_search_matches_title_and_description`
- `the_claims_listing_honours_the_pagination_bounds`
- `a_currency_filter_that_is_not_three_letters_is_refused` **(new)** — includes `?currency=`
- `a_search_term_over_the_bound_is_refused` **(new)** — 201 characters, `422` naming `search`
- `the_claim_detail_items_are_in_chronological_order` (the items are written as a SQL fixture; the assertion is on the response order)

### 12.7 `tests/claims_api.rs` — the credential

- `the_claim_routes_require_a_credential` (five operations, no token and an invalid token, `401 unauthorized` with the standard envelope)

### 12.8 `tests/openapi_contract.rs` — the published document

- `each_list_endpoint_publishes_its_filter_and_pagination_parameters_once` (extended with `GET /api/v1/claims` → `limit`, `offset`, `status`, `currency`, `search`, `submitted_from`, `submitted_to`)
- `the_claim_money_fields_are_strings_in_the_document` **(new)** — `ClaimResponse.total_amount` and `ItemResponse.amount` are `type: string` with the four-decimal pattern, never `type: number`
- `the_update_claim_request_publishes_a_nullable_project_id` **(new)** — the `Option<Option<Uuid>>` seam is visible in the document as a nullable string/uuid, not as a missing field
- `the_route_table_and_the_document_describe_the_same_operations` and
  `every_route_in_the_table_is_served_by_the_router` — unchanged tests that now
  cover the five new rows because `ROUTE_TABLE` gains them in the same change

### 12.9 Unit tests — inside the module

| File | Test | What it proves |
| ---- | ---- | -------------- |
| `dto.rs` | `a_project_link_is_kept_cleared_or_set_by_the_three_body_forms` **(new)** | three JSON bodies deserialized through `serde_json::from_str` produce `(false, None)`, `(true, None)` and `(false, Some(id))` — the `#[serde(default)]` and `double_option` pair of §5.2 cannot collapse `null` into "absent" |
| `service.rs` | `a_currency_is_three_letters_and_is_upper_cased` | `"eur"` → `"EUR"`; `"EU"`, `"EURO"`, `"1US"`, `""` refused |
| `service.rs` | `a_reversed_submission_range_is_refused` **(new)** | the `422` names `submitted_from`; equal bounds are accepted |
| `service.rs` | `a_blank_search_means_no_filter` **(new)** | `"   "` becomes `None`, not a filter on spaces |
| `service.rs` | `a_claim_title_is_trimmed_before_it_is_measured` **(new)** | 200 characters plus surrounding spaces is accepted and stored trimmed |

### 12.10 The rule-to-test map

| Rule | Test |
| ---- | ---- |
| create returns `draft`, `"0.0000"`, `items: []` | `a_claim_can_be_created_read_and_deleted` |
| the listing carries no items | `a_claim_can_be_created_read_and_deleted`, `the_claim_list_returns_the_callers_claims_newest_first` |
| a project link must be the caller's, on create | `a_claim_cannot_reference_another_users_project` |
| a project link must be the caller's, on patch | `a_claim_cannot_reference_another_users_project` |
| a deleted project leaves the claim | `deleting_the_project_clears_the_link_and_keeps_the_claim` |
| every verb is owner-scoped, `404` not `403` | `a_claim_of_another_user_is_invisible`, `the_listing_only_contains_the_callers_claims` |
| the listing's envelope counts the filtered set | `the_claims_listing_filters_by_status_and_currency` (+ the date test) |
| order is `created_at DESC, id DESC` | `the_claim_list_returns_the_callers_claims_newest_first` |
| the clamp and the offset | `the_claims_listing_honours_the_pagination_bounds` |
| the date bounds are inclusive and exclude drafts | `the_claims_listing_filters_by_submission_dates` |
| the reversed range is `422` | `a_reversed_date_range_is_refused` |
| an omitted PATCH field is untouched | `a_claim_can_be_partially_updated` |
| `"project_id": null` clears | `a_claim_can_be_partially_updated` |
| an empty PATCH body changes only `updated_at` | `a_patch_body_with_no_fields_changes_only_the_modification_time` |
| `currency`/`status` are ignored, not rejected | `the_claim_currency_is_not_patchable`, `the_claim_status_is_not_patchable` |
| the total is server-owned | `the_total_is_not_accepted_from_the_client` |
| money is a four-decimal string on the wire | `money_round_trips_with_four_decimals` + `a_money_field_is_never_a_json_number_in_a_response` + `the_claim_money_fields_are_strings_in_the_document` |
| `title`/`description`/`currency` rules | the §12.4 block |
| `400` for a bad path, a bad body, a bad query value | the §12.5 block |
| `401` on every route | `the_claim_routes_require_a_credential` |
| `413` on a large body | `a_claims_body_over_the_limit_is_refused` |
| the document and the router agree | the two existing `openapi_contract` tests |
| the published parameter set is exact | `each_list_endpoint_publishes_…` |

**Every test must be able to fail for the rule it names.** The
`R2` change has to show, for each of the five security-relevant rules — the
claim predicate, the project predicate, the ordering, the `404`-not-`403`
mapping and the `status`/`currency` non-writability — that removing or inverting
the rule makes its test fail, then restore it.

---

## 13. Files that will change

| Owner | Files |
| ----- | ----- |
| `rust-backend` | `src/reimbursements/{mod,dto,service,repository,handlers}.rs`, `src/state.rs`, `src/api/routes.rs`, `src/helpers/mod.rs` (only if a re-export is genuinely needed), `config/messages.toml`, `src/config/messages.rs` |
| `test-engineer` | `tests/claims_api.rs` (new), `tests/openapi_contract.rs`, `tests/common/mod.rs` (only if a `create_claim` helper is genuinely missing) |
| `docs` | `docs/features/reimbursements/{api,architecture,testing,status}.md` (the §9.1 corrections and the "planned" markers), `docs/api/README.md` if it indexes the operations, `docs/engineering/implementation-status.md` |
| `database` | `docs/database/schema.md` §11.1 (the §9.1 and §9.3 corrections) |
| orchestrator | `docs/engineering/{change-log,review-log,implementation-status}.md`, `docs/engineering/runs/archive/phase-2/ledger.md` (`C-R2`, the checkpoint, the `C-R1` `I6` correction) |

No migration, no `Cargo.toml` change, no configuration setting.

---

## 14. Acceptance criteria for a reviewer

1. **Endpoints.** Exactly five operations exist; `ROUTE_TABLE` and the OpenAPI
   document agree in both directions; the list operation publishes the seven
   parameters once each; the `bearer` scheme applies to all five.
2. **Ownership.** Every statement — nine of them, behind six methods — carries
   `user_id` in its own `WHERE`; a `grep` for a statement in `repository.rs`
   finds none without it; a foreign claim, and a foreign `project_id` on create
   and on patch, is `404` with code `not_found`; no `403` appears anywhere in the
   module.
3. **`404` versus `422`.** A malformed body is validated before any statement
   runs; a bad path parameter and an unparsable query parameter are `400`; an
   unknown `status` value is `400`; a business rule is `422` with the field
   named.
4. **PATCH.** An omitted field is untouched in the SQL as well as in the DTO
   (`COALESCE`); `null` clears only `project_id`; `currency`, `status` and
   `total_amount` are absent from `UpdateClaimRequest` and from every `SET`
   clause; an empty body returns `200`.
5. **Money.** No money field is a number on the wire; every money select is
   `::text`; there is no `Money` type, no decimal crate, no float; the four
   decimals of a response come from the column's rendering.
6. **Listing.** The order is `created_at DESC, id DESC`; the two index shapes
   are the two statements; the count matches the filters; the date bounds are
   UTC half-open days; a draft is excluded when a bound is present; the clamp
   comes from `PaginationQuery`.
7. **No scope creep.** No item endpoint, no lifecycle action, no `409`, no
   transaction, no new dependency, no new setting, no migration.
8. **Documentation.** The §9.1 and §9.3 corrections are in, the feature
   documents record what `R2` built and what `R3`–`R5` still owe, and nothing is
   marked `Verified` without a recorded run.

---

## 15. Alternatives rejected

| Alternative | Why not |
| ----------- | ------- |
| A `Money` newtype in `dto.rs`, or a `src/money.rs` module | §4.3. It would be a second wire contract with no method but a constructor, and it cannot name a field better than the extractor does. The plan in [`module-boundaries.md`](module-boundaries.md) is narrowed, not reversed: if a typed money value is ever genuinely needed, `src/money.rs` is still where it goes. |
| Validating money by relying on the `NUMERIC(19,4)` column | §4.4. The column coerces `"12.5"` to `12.5000` and **rounds** a fifth decimal, so the contract's "no rounding, exactly four decimals" would be false. |
| Letting the `status` filter be an optional predicate like the others | It would hide the filter from the planner and leave `expense_claims_user_id_status_created_at_idx` unread. Two statement shapes cost four constants and buy the index the schema was built for. |
| One disjunction-heavy statement for every filter combination | 32 combinations, or one statement the planner cannot use. The two-shape split is the smallest set that uses both indexes. |
| A plain `Option<Option<Uuid>>` with no `deserialize_with` | `serde` answers an explicit `null` with `visit_none` on the outer `Option`, so `{}` and `{"project_id": null}` both deserialize as `None` and "clear the link" becomes impossible to express. §5.2 gives the eight-line helper instead. |
| `serde_with::rust::double_option` | It is the same helper, in a new dependency, for one field. `AGENTS.md` §4.8 requires a concrete reason for a crate; eight lines in `dto.rs` is the smaller change. |
| Reading the project first, then updating the claim | A second round trip and a check-then-act race on the success path. The check belongs in the statement. |
| Returning `409` when a `PATCH` arrives on a non-draft claim | Not in the contract: `REIMB-R06` restricts item writes, not claim metadata, and `api.md` §6 lists no `409`. |
| `#[serde(deny_unknown_fields)]` on the update DTO, to make `currency` an error | It turns every unknown key into a `422`, which is a breaking change and contradicts the "ignores extra keys" rule of §9.1. |
| One statement for the detail read using `json_agg` | It needs the `sqlx` `json` feature — a new dependency in all but name — and a JSON-in-a-column shape in the row struct. Two owner-scoped reads are cheaper and the snapshot caveat is stated (§6.7). |
| A `total` computed in Rust from the items | `ADR-0007`: the sum is `SUM` in SQL, and no arithmetic exists in the request path. `R2` does not compute it at all. |
| Persisting `submitted_from`/`submitted_to` as timestamps | The contract says dates; converting a date to a timestamp in the query keeps the request's meaning ("the whole day in UTC") in the statement. |

---

## 16. Related decisions and documents

| Reference | Relationship |
| --------- | ------------ |
| [`../features/reimbursements/api.md`](../features/reimbursements/api.md) | The published contract this design implements. |
| [`../features/reimbursements/specification.md`](../features/reimbursements/specification.md) | `REIMB-R01`…`R11`, and the acceptance criteria the tests serve. |
| [`../features/reimbursements/testing.md`](../features/reimbursements/testing.md) | The suite, including the `R2` subset named in §12. |
| [`../database/schema.md`](../database/schema.md) §11.1–§11.2 | The columns, constraints and indexes `R2` reads and writes. |
| [`../../migrations/0004_expense_claims.sql`](../../migrations/0004_expense_claims.sql) | The enforced copy of §11. |
| [`../../tests/claims_schema.rs`](../../tests/claims_schema.rs) | What the schema already guarantees, so `R2` does not re-check it. |
| [`ADR-0005`](../decisions/ADR-0005-ownership-enforced-in-sql.md) | The ownership predicate in every statement, and `404` rather than `403`. |
| [`ADR-0007`](../decisions/ADR-0007-money-representation.md) | `NUMERIC(19,4)`, a four-decimal JSON string, no arithmetic in Rust. |
| [`../api/conventions.md`](../api/conventions.md) §5, §7, §15, §16 | Pagination, PATCH, money, filtering and ordering. |
| [`../api/error-contract.md`](../api/error-contract.md) | Every status, every code, and the `400`/`422` split. |
| [`module-boundaries.md`](module-boundaries.md) | Where the module sits, what it may depend on, and the narrowed `money` plan. |
| [`../../AGENTS.md`](../../AGENTS.md) §4 | The engineering rules this design is held to. |

**ADR position.** No new ADR is written for this chain, and none is needed.
`R2` adds endpoints that fit the accepted model — which
[`README.md`](README.md) §3 explicitly excludes from the ADR requirement — and
it changes no accepted decision: the money representation is `ADR-0007`'s and is
untouched; the ownership rule is `ADR-0005`'s and is applied, not amended; the
error contract is unchanged. The one structural change — that `R2` builds no
money type, and that the four-decimal check will be a validator in `helpers`
rather than a type in `dto.rs` or a `src/money.rs` — narrows a **plan** in
[`module-boundaries.md`](module-boundaries.md), and is recorded there in the
same change, because that document owns the plan. A future change that needs
money handled in Rust needs an ADR superseding `ADR-0007`.
