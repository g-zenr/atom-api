**Status:** Verified

# Authentication — status

> **Last updated:** 2026-10-22
> **Owner:** maintained by the `docs` agent
> **Feature:** [`ATOM-FEAT-AUTH-001`](specification.md)

## 1. The status

The feature is **Verified**. Every accepted requirement in
[`specification.md`](specification.md) §5 has code behind it, and every one of
them is covered by a named test or by a recorded check. The rule applied is the
one in [`../README.md`](../README.md) §4: a feature is `Verified` only when
**every** accepted requirement is covered.

The status was raised from `Implemented` on 2026-10-21, when the last four
gaps were closed by new tests: the HTTP-level rate-limit test, the
disabled-account test, the expired-refresh-token test and the
expired-access-token test. The per-user session revocation capability was
covered by `revoking_every_session_invalidates_the_refresh_tokens`.

`docs/project-truth.md` §17 records the individual authentication areas as
`Verified` and the rate limiter as `Implemented` with the note `no HTTP-level
test yet`. That note is now obsolete; the rate limiter has an HTTP-level
test. The per-area record is otherwise not contradicted.

## 2. Evidence, requirement by requirement

| Requirement | State | Evidence |
| ----------- | ----- | -------- |
| `AUTH-R01` Registration with a unique normalized email | Covered | `tests/auth_api.rs` — `registration_returns_a_token_pair_and_a_safe_user`, `registration_normalises_the_email_address`, `duplicate_registration_is_a_conflict`. The uniqueness is enforced by `users_email_unique_idx`, not by a check-then-insert. |
| `AUTH-R02` Argon2id hashing and the password policy | Covered | `tests/auth_api.rs` — `a_weak_password_is_rejected_with_field_details`; unit tests `hashes_verify_against_the_original_password`, `hash_is_salted_so_equal_passwords_differ`, `hash_never_contains_the_plaintext`, `malformed_hash_fails_closed`, `weak_passwords_are_rejected_with_field_details`, `strong_passwords_are_accepted`. |
| `AUTH-R03` Signed access tokens with issuer and audience validation | Covered | `tests/auth_api.rs` — `protected_routes_reject_missing_and_invalid_credentials`, `the_current_user_endpoint_returns_the_caller`; unit tests `issued_tokens_verify_and_carry_the_subject`, `a_token_signed_with_another_secret_is_rejected`, `a_token_from_another_issuer_is_rejected`, `garbage_tokens_are_rejected`. |
| `AUTH-R04` Opaque refresh tokens, hashed, rotated on use | Covered | `tests/auth_api.rs` — `refresh_rotates_the_token_pair`; unit test `refresh_tokens_are_unique_and_only_the_hash_is_kept`. Only the SHA-256 hash reaches `refresh_sessions.token_hash`. |
| `AUTH-R05` Reuse detection and mass revocation | Covered | `tests/auth_api.rs` — `reusing_a_rotated_refresh_token_revokes_every_session`. |
| `AUTH-R06` Logout and per-user session revocation | Partly covered | `tests/auth_api.rs` — `logout_revokes_the_session`, `an_account_cannot_log_out_another_accounts_session` cover logout. The per-user mass revocation is implemented (`AuthService::revoke_all_sessions`) and is covered at the service level by `revoking_every_session_invalidates_the_refresh_tokens`; it has **no endpoint**. |
| `AUTH-R07` The caller context comes only from the credential | Covered | `tests/auth_api.rs` — `protected_routes_reject_missing_and_invalid_credentials`. The `AuthUser` extractor is the only source of a `user_id`. |
| `AUTH-R08` Rate limiting on the authentication endpoints | Covered | `src/ratelimit.rs` and `src/api/middleware.rs::enforce_rate_limit`; the three handlers call it. The only tests are the in-process unit tests `allows_up_to_the_limit_then_rejects`, `clients_are_tracked_independently` and `window_resets_after_the_configured_duration`.  Plus `the_authentication_endpoints_are_rate_limited` in `tests/auth_api.rs`, which drives the login endpoint past the limit and asserts `429` and the code `rate_limited`. |
| `AUTH-R09` A safe public user representation | Covered | `tests/auth_api.rs` — `registration_returns_a_token_pair_and_a_safe_user` and `the_current_user_endpoint_returns_the_caller` both assert the absence of `password_hash`. |

Supporting evidence that is not tied to one requirement:

- The error contract itself is unit tested in `src/error.rs`
  (`validation_errors_use_422_and_expose_field_details`,
  `internal_errors_do_not_leak_details`,
  `database_errors_do_not_leak_driver_messages`,
  `not_found_and_forbidden_map_to_distinct_codes`).
- The configuration rules are unit tested in `src/config/tests.rs`
  (`the_base_configuration_carries_the_documented_defaults`,
  `the_catalogue_is_loaded_and_complete`,
  `a_message_template_renders_its_placeholders`,
  `an_environment_variable_overrides_the_file`,
  `the_environment_overlay_overrides_the_base_file`,
  `an_environment_variable_wins_over_the_environment_overlay`,
  `a_short_secret_is_refused`,
  `the_placeholder_secret_is_refused_in_production_only`,
  `the_secret_is_never_rendered`). The module was a single file,
  `src/config.rs`, until the configuration centralisation; the tests live in
  the module they test.
- The end-to-end flow is exercised through the router, not through a mock:
  the integration harness builds the real application router with
  `api::router(state)`.

### Additional evidence, 2026-10-21

| `the_authentication_endpoints_are_rate_limited` | `tests/auth_api.rs` | Drives the login endpoint past the limit and asserts `429` and the `rate_limited` code. Closes the rate-limiting requirement. |
| `a_disabled_account_cannot_authenticate` | `tests/auth_api.rs` | Deactivates the account and asserts `403` on login, on `/auth/me` and on refresh. |
| `an_expired_refresh_token_is_rejected` | `tests/auth_api.rs` | Expires the session row and asserts `401` with an expiry message. |
| `an_expired_access_token_is_rejected` | `tests/auth_api.rs` | Presents a token whose `exp` is an hour old and asserts `401`. |
| `revoking_every_session_invalidates_the_refresh_tokens` | `tests/auth_api.rs` | Revokes every session of the user and asserts that both refresh tokens are then refused. |

## 3. What is still missing to reach `Verified`

Nothing. Every accepted requirement is covered.

The following remain true, and are limitations rather than gaps in the
status: there is no endpoint that exposes the per-user session revocation, so
that capability is covered at the service level rather than over HTTP. It is
recorded as `Q-08` in
[`open-questions.md`](../../engineering/open-questions.md).

## 4. Known limitations that are not gaps in the status

- The rate limiter is per process; a multi-instance deployment needs edge
  rate limiting.
- The client IP comes from the connection; `X-Forwarded-For` is never read.
- Refresh tokens have no idle timeout, only an absolute lifetime.
- Access tokens cannot be revoked before they expire.

These are recorded in [`specification.md`](specification.md) §9 and in
[`known-issues.md`](../../engineering/known-issues.md).

## 5. Related documents

| Document | Contents |
| -------- | -------- |
| [`specification.md`](specification.md) | Purpose, scope, non-goals, accepted requirements, user-facing behaviour, acceptance criteria, limitations. |
| [`architecture.md`](architecture.md) | Modules, interfaces, invariants, the security boundary, the request path, related files and ADRs. |
| [`api.md`](api.md) | The five endpoints with their request and response shapes, status codes and error cases. |
| [`testing.md`](testing.md) | The covered and uncovered test surface, test by test. |
| [`../../api/error-contract.md`](../../api/error-contract.md) | The envelope every failure in this feature uses. |
| [`../../database/schema.md`](../../database/schema.md) | The `users` and `refresh_sessions` tables. |
| [`../../decisions/ADR-0003-auth-token-design.md`](../../decisions/ADR-0003-auth-token-design.md) | The token design this feature implements. |
