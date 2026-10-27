**Status:** Implemented

# API keys — status

> **Last updated:** 2026-10-27
> **Owner:** maintained by the `docs` agent
> **Feature:** the machine credential of `K1`; the accepted decision is
> [`ADR-0011`](../../decisions/ADR-0011-api-keys.md)
> **Read this first:** the three operations are published, and their cases pass
> in the measured run of 2026-10-27. The feature is still **`Implemented`**,
> because an independent review of the diff and a quality-gate verdict are what
> `Verified` requires, and neither is recorded. §3 is that sentence in full.

This is a **card**, not a five-document feature set: the capability is one
credential and three operations, and its design is recorded once in
[`ADR-0011`](../../decisions/ADR-0011-api-keys.md). It carries no `ATOM-FEAT-`
identifier, because no requirement in
[`../../project-truth.md`](../../project-truth.md) defines a machine credential;
the authority for it is the accepted ADR alone.

## 1. What exists

Read from the tree on 2026-10-27.

| Piece | State | Evidence |
| ----- | ----- | -------- |
| `POST /api/v1/api-keys` | **Published.** `201`, and it is the only response that carries the secret. | `src/api_keys/handlers.rs` |
| `GET /api/v1/api-keys` | **Published.** `200`, paginated, newest first; the response shape has no field for the secret or the digest. | `src/api_keys/handlers.rs`, `src/api_keys/dto.rs` |
| `DELETE /api/v1/api-keys/{key_id}` | **Published.** `204`; idempotent, so a second revoke is `204` too and keeps the first instant; an unknown or foreign id is `404`. | `src/api_keys/handlers.rs`, `src/api_keys/service.rs` |
| The credential | **`atom_` + 32 CSPRNG bytes rendered as lower-case hex = 69 characters.** Stored only as a SHA-256 hex digest; `prefix` is `atom_` + the first 8 hex characters; `last_used_at` is written at most once per hour per key. | `src/api_keys/secret.rs`, `src/api_keys/service.rs` |
| The middleware branch | **Exists.** `AuthUser` switches on the `atom_` prefix alone, so a key never reaches the JWT decoder and a malformed `atom_…` value is a credential failure rather than a decode error. | `src/api/middleware.rs` |
| The schema | **Exists.** `migrations/0007_api_keys.sql`: `revoked_at IS NULL` means active, `ON DELETE CASCADE` from `users`, one index on `user_id`. | [`../../database/schema.md`](../../database/schema.md) §11.7 |
| The surface | **54 operations**: 52 under `/api/v1` and the two health probes. `CONTRACT_OPERATIONS = 54`. | `src/api/routes.rs`, `tests/openapi_contract.rs` |

The three failure answers, which are the contract a client switches on:

| Situation | Answer |
| --------- | ------ |
| Unknown, malformed or revoked key | `401` `unauthorized` |
| The key of an inactive account | `403` `forbidden` `account_disabled` — the same answer that account's JWT gets (amendment `A-C-K1-1`) |
| Revoking a key that is not the caller's, or does not exist | `404` `not_found`, never `403` |

All three operations require a bearer credential, and both kinds may call them:
a user access token and an API key are equally able to manage keys. That is
deliberate — it is what lets a workflow rotate its own credential — and it is
the reason limitation 1 in §4 is real.

## 2. The evidence

| Evidence | Result |
| -------- | ------ |
| [`../../../tests/api_keys_api.rs`](../../../tests/api_keys_api.rs) | **11 cases, all pass** in the run of 2026-10-27: the ten cases the contract names (returned once and never again, authentication, revoked, inactive account, foreign key, listing without a secret, malformed, cross-account management, the hourly stamp, owner scoping) plus one repository-level case that proves the usage statement refuses a second write inside the hour on its own. |
| Unit tests in `src/api_keys/` | **13 cases**: `secret.rs` 5, `models.rs` 3, `service.rs` 4, `dto.rs` 1. |
| The contract assertions | `tests/contract_sweep.rs` and `tests/openapi_contract.rs` assert the three routes and pin the operation set at 54. |
| The six gates | `scripts/gate.ps1`: all six at `0` on 2026-10-27. |
| The suite | `cargo test --all-targets --locked`: **536 passed, 0 failed** on Windows at this tree. CI's Linux runner reports 538, because the two unix-only `shutdown` cases run there. |

A green suite is not a review. Nothing in this section is a reviewer's opinion
of the code.

## 3. What `Verified` would need

Both records now exist, and they disagree, which is why the status is still
`Implemented`:

1. **The quality-gate verdict** is **PASS** and is recorded in
   [`../../engineering/review-log.md`](../../engineering/review-log.md) as
   `ATOM-REVIEW-20261027-001`: six gates at `0`, 536 tests, 54 operations
   asserted in both directions.
2. **The independent review** of the diff is **FAIL on documentation**, three
   reads, with no ownership, SQL, secret-handling or contract defect found in
   the code. Every finding is repaired, and no read of the final tree exists.

Until that last read is written down, the status is `Implemented` as
[`AGENTS.md`](../../../AGENTS.md) §6 defines it, and no document may say
otherwise.

## 4. Limitations

These are accepted, not defects of the description; `KI-024` in
[`../../engineering/known-issues.md`](../../engineering/known-issues.md) records
the first three together.

1. **A key has no scope.** It carries the full authority of its owner, on every
   private operation — including `POST /api/v1/api-keys` itself, so a key can
   mint another key. A leaked key is a full-account compromise until somebody
   revokes it.
2. **There is no expiry.** A key is valid until `DELETE /api/v1/api-keys/{key_id}`
   is called; the schema has no `expires_at`.
3. **`last_used_at` is coarse.** It is written at most once an hour per key, so a
   use inside the hour of the previous stamp is not recorded. It is the only
   audit signal the credential has.
4. **Revoking a leaked key does not end access it created.** A key that minted a
   second key leaves that second key alive; revoking the parent revokes only the
   parent's row.

## 5. Related documents

| Document | Contents |
| -------- | -------- |
| [`../../decisions/ADR-0011-api-keys.md`](../../decisions/ADR-0011-api-keys.md) | The decision, the credential format, why SHA-256 rather than Argon2, and the accepted negatives. |
| [`../../api/README.md`](../../api/README.md) | The three operations in the published surface, which is 54 operations. |
| [`../../database/schema.md`](../../database/schema.md) | The `api_keys` table, §11.7. |
| [`../../architecture/security-architecture.md`](../../architecture/security-architecture.md) | The authentication boundary the branch sits on, §2. |
| [`../../engineering/implementation-status.md`](../../engineering/implementation-status.md) | The `K1` row, with the same evidence and date. |
| [`../../engineering/known-issues.md`](../../engineering/known-issues.md) | `KI-024`: the authority, the missing expiry and the mint chain. |
| [`../README.md`](../README.md) | The feature index and the status vocabulary. |
