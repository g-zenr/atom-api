# ADR-0007 — Money is `NUMERIC(19,4)` and a JSON string

> **Status:** Accepted
> **Date:** 2026-10-24
> **Owner:** architect

## Context

Phase 2 introduces money into the API for the first time: a reimbursement
claim has a total and a set of line-item amounts, and a job application may
carry a salary range. The requirement is `REIMB-R07` and `JOB-R03` in
[`../project-truth.md`](../project-truth.md) §18.1 and §18.2.

Three properties have to hold at once:

1. **The total is exactly the sum of the parts.** `REIMB-R03` says
   `total_amount` always equals the sum of the item amounts. A representation
   that loses a unit in the last place cannot satisfy it.
2. **The value survives the round trip unchanged.** A client sends an amount,
   reads it back, and must get the same digits — not `12.499999999` for
   `12.5000`.
3. **The contract is unambiguous to every client.** ATOM has no client yet,
   and the eventual web and mobile clients are in different languages with
   different default number types.

The representation was not settled by an owner decision. It is provisional
default **A6** in
[`../engineering/runs/chain/plan.md`](../engineering/runs/chain/plan.md) §3,
which the owner did not explicitly confirm (owner decision **O1** in
[`../engineering/runs/chain/ledger.md`](../engineering/runs/chain/ledger.md)).
This ADR records the default as a decision, so that the implementing change
has one answer rather than a choice per module. It is provisional in the sense
that the *value* can be revised by the owner; the *decision* is in force until
an ADR supersedes it.

PostgreSQL's `numeric` type is an exact decimal with a declared precision and
scale. `NUMERIC(19,4)` allows 15 integer digits and exactly 4 fractional
digits, so the largest magnitude it can hold is
`999999999999999.9999`. That is large enough for a personal expense claim and
for any realistic salary, and it is the shape the plan's data model sketch
already uses.

## Decision

**ATOM stores money in PostgreSQL as `NUMERIC(19,4)`, transports it in JSON as
a string with exactly four decimal places, and performs no floating-point
arithmetic on it anywhere.**

### 1. The database

- Every money column is `NUMERIC(19,4)`. No money column is `REAL`, `DOUBLE
  PRECISION`, `MONEY` or `integer`.
- Amounts are non-negative and are constrained in the database
  (`CHECK (amount >= 0)`), so an out-of-contract value cannot be persisted
  even if the service is bypassed.
- The claim total is computed **by PostgreSQL** with `SUM(amount)` over the
  claim's items, in the same transaction as the item write. The application
  never adds money.
- Salary bounds are compared **by PostgreSQL**
  (`CHECK (salary_min <= salary_max)`), not by the application.

### 2. The wire format

- A money value is a JSON **string** with exactly four fractional digits:
  `"125.5000"`, `"0.0000"`, `"999999999999999.9999"`.
- A JSON **number** where money is expected is a `422 validation_failed`
  naming the field. It is not silently accepted, because `12.5` as a JSON
  number has already lost the information that the contract requires.
- A string with a different number of decimals (`"12.5"`, `"12.500"`) is a
  `422` naming the field. The four decimals are the contract, not a
  suggestion, so that a client can compare and render without guessing.
- A negative amount is a `422`; a value above the maximum is a `422`.
- The currency is a separate field, ISO 4217, three uppercase letters. A
  money value never carries its own currency.

### 3. The Rust representation

- Money crosses the repository boundary as a `String`, selected with an
  explicit cast:

  ```sql
  SELECT total_amount::text FROM expense_claims WHERE id = $1 AND user_id = $2
  ```

  PostgreSQL renders `numeric` in its canonical form with the declared scale,
  so the string the client receives is the exact stored value.
- Validation is a character-level check of the textual form: an optional
  leading `-` (rejected for amounts), at most 15 integer digits, a `.`, and
  exactly 4 digits. The service never parses the value into a float.
- No new crate is added for this. If a later requirement needs application-side
  arithmetic — a pro-rated split, a multi-currency total — that change adds
  `rust_decimal` and records the reason in the change log, and this ADR is
  amended rather than ignored.

## Alternatives considered

| Alternative | Why it was not chosen |
| ----------- | --------------------- |
| **`f64` / `DOUBLE PRECISION` / a JSON number** | Binary floating point cannot represent `0.1`, `0.2` or `12.5` exactly, so `0.1 + 0.2 != 0.3` and a sum drifts in the last places. It also admits `NaN` and `Infinity`, which a JSON response cannot represent. The requirement that the total equals the sum of the parts would hold only "usually", which is not a requirement. The rejected design is the one most likely to be reached for by accident, which is why the decision is written down. |
| **Integer minor units (`i64` cents)** | Exact, and a common answer. It was not chosen because (a) four decimal places are not "cents" in every currency — JPY has no minor unit, KWD has three — so the scale would be a hidden, currency-dependent convention; (b) it forces every client to convert, and a conversion is exactly where a rounding bug appears; (c) `i64` covers `±9.2e18`, which is *less* than `NUMERIC(19,4)`'s `±9.99e17` scaled by 10 000, so the range would shrink; and (d) the database column would then disagree with the type the plan and the schema document use. |
| **`rust_decimal` in the application, `NUMERIC` in the database** | Exact and ergonomic, and it is where this decision would go if the application needed arithmetic. It was not chosen for Phase 2 because it adds a dependency (which `AGENTS.md` §4.8 requires a recorded reason for) to perform arithmetic that PostgreSQL already performs exactly, and because a second numeric type in the request path is a second place for a conversion to go wrong. |
| **A `MONEY` column** | PostgreSQL's `money` type is locale-dependent: its input and output formats depend on `lc_monetary`, so the same statement behaves differently on two servers. That is unacceptable for a value that a test asserts byte for byte. |
| **Text without a numeric column** | It would avoid the type question entirely, but it cannot be summed, compared or constrained, so `REIMB-R03` and `JOB-R03` would have to be enforced in application code. Rejected. |
| **Storing a currency per line item as well as per claim** | `REIMB-R02` says one currency per claim, and no FX conversion is in scope. A per-item currency would invite a mixed-currency claim that no requirement asks for. |

## Consequences and trade-offs

### Positive

- **The sum is exact.** `SUM(numeric)` is exact, so `REIMB-R03` is a database
  guarantee rather than a hope.
- **The round trip is lossless.** The string the client sends is the string it
  reads back, byte for byte, which makes the contract testable without a
  tolerance.
- **No new dependency.** No decimal crate, no float, nothing to add to
  `Cargo.toml`.
- **The database is the enforcer.** Non-negativity, the range and the salary
  ordering are `CHECK` constraints, not only service rules.
- **The client cannot accidentally lose precision**, because there is no
  number to parse into a float.

### Negative

- **Every client must handle money as a string.** A JavaScript client that
  does `Number(amount)` loses the guarantee, and nothing in the API can stop
  it. The convention therefore has to be stated prominently in
  [`../api/conventions.md`](../api/conventions.md) and in every feature
  `api.md`.
- **Four decimals for every currency.** A JPY amount is rendered `"1500.0000"`
  and a KWD amount keeps four decimals where three would be conventional. The
  scale is a property of the storage, not of the currency. Accepted: the
  alternative is a per-currency scale, which is a table and a rule that no
  requirement asks for.
- **The `::text` cast is a discipline, not a compiler check.** A query that
  selects a `numeric` column without the cast asks SQLx for a type it does not
  decode by default, and the failure appears when the query runs. This is the
  same run-time-checking trade-off as AR-3 in
  [`../security/threat-model.md`](../security/threat-model.md), and it is the
  price of not adding a decimal crate.
- **No arithmetic in the application.** A feature that needs to compute a
  share of a claim, or a converted total, cannot do it with this design and
  will have to revisit this ADR. That is deliberate: the need does not exist
  yet.
- **The range is finite.** `NUMERIC(19,4)` cannot represent cryptocurrency
  amounts or hyperinflation figures. ATOM tracks personal expenses and
  salaries, so this is out of scope rather than a surprise.

## Security and operational implications

- **No secret is involved.** Money is owner data, not credential material.
- **Injection surface is unchanged.** The value is a bound parameter (`$n`)
  like every other; the string form does not change that. The database casts
  it to `numeric` and rejects a malformed value with SQLSTATE `22P02`, which
  the service maps to a `422` naming the field rather than to a `500`.
- **A malformed value cannot crash the process.** The application validates
  the textual form before the statement runs, and performs no `unwrap` on a
  parse.
- **The value appears in logs only if a handler logs it.** No money value is
  written to a log line by the framework; the feature adds no logging of
  request bodies.
- **Operationally there is nothing to configure.** The precision and scale are
  in the migration, not in `config/`. Changing them later is a migration and a
  breaking contract change, not a setting.

## Related requirements and changes

| Reference | Relationship |
| --------- | ------------ |
| [`../project-truth.md`](../project-truth.md) §18.1 `REIMB-R03`, `REIMB-R07` | The requirements this decision serves. |
| [`../project-truth.md`](../project-truth.md) §18.2 `JOB-R03` | The salary range that uses the same representation. |
| [`../project-truth.md`](../project-truth.md) §18.4 | Records A6 as a provisional default. |
| [`../api/conventions.md`](../api/conventions.md) §15 | The published money convention. |
| [`../features/reimbursements/api.md`](../features/reimbursements/api.md) | The endpoints that carry a money string. |
| [`../features/reimbursements/architecture.md`](../features/reimbursements/architecture.md) | Where the sum is computed. |
| [`ADR-0002-rust-axum-postgresql.md`](ADR-0002-rust-axum-postgresql.md) | The stack this decision builds on. |
| [`../engineering/change-log.md`](../engineering/change-log.md) | The change that implements it. |

## Supersedes / Superseded by

Supersedes: none. No earlier decision covered the representation of money.

Superseded by: none. A review of this decision is due when a requirement needs
application-side arithmetic on money, when a second currency must coexist in
one claim, or when the owner revises provisional default A6.
