# ADR-0011 — A machine client authenticates with a per-user, revocable API key

> **Status:** Accepted
> **Date:** 2026-10-27
> **Owner:** architect

## Context

ATOM has exactly one way to identify a caller. `api::middleware::AuthUser`
requires `Authorization: Bearer <token>` and validates an HS256 JWT with a
**15-minute** lifetime (`auth.access_token_ttl_seconds = 900` in
`config/auth.toml`); the only long-lived credential the API issues is a refresh
token, which is opaque, valid for 14 days, **rotated on every use** and
protected by **reuse detection**
([`ADR-0003-auth-token-design.md`](ADR-0003-auth-token-design.md),
[`../architecture/security-architecture.md`](../architecture/security-architecture.md)
§2.3).

The owner is integrating their deployed n8n instance with ATOM. An n8n workflow
runs unattended: it must authenticate on every scheduled run, with no human to
log in, and it must not be able to disturb the owner's own sessions. Neither
existing credential can serve that, and the failure is concrete rather than
theoretical:

1. **A workflow cannot mint a 15-minute access token.** The only ways to obtain
   one are `POST /api/v1/auth/login` (a password stored in the workflow, and a
   route rate limited by client address) or `POST /api/v1/auth/refresh`. A
   workflow that refreshes on a schedule *is* the rotating client described
   next.
2. **Rotation breaks a stored refresh token.** `POST /api/v1/auth/refresh`
   revokes the presented session, inserts a replacement and links the two
   through `replaced_by_session_id`. A workflow that keeps the token it was
   given holds a token that is dead the first time anything else refreshes it,
   and a workflow that refreshes on every run invalidates the token any other
   actor holds.
3. **Reuse detection turns a retry into a lockout.** Presenting an
   already-revoked refresh token is treated as reuse: **every** active session of
   that account is revoked
   ([`../architecture/security-architecture.md`](../architecture/security-architecture.md) §2.3).
   A crash-restore, a retried HTTP request, or two workflow branches sharing one
   stored credential therefore signs the human owner out of their own workspace
   and revokes every session they have. The control that protects a person's
   refresh token is precisely the control that punishes an automation.
4. **Doing nothing is worse than either.** Putting the owner's password in the
   workflow would work, and it is rejected: it cannot be revoked without changing
   the password (which signs out every device), and it grants the ability to log
   in as the owner from anywhere, not only to call the API.

What the integration needs is a credential a workflow can hold, that does not
rotate under it, that is scoped to exactly one account, and that can be retired
without touching the person's sessions or the token signing secret.

## Decision

ATOM authenticates a machine client with a **per-user, revocable API key**.

**Form.** `atom_` followed by 32 bytes from the operating system CSPRNG,
rendered as 64 lower-case hexadecimal characters. `atom_` is **five** characters,
so a secret is **69** characters. The length is derived from the byte count in
`src/api_keys/secret.rs` (`KEY_PREFIX.len() + SECRET_BYTES * 2`) rather than
written down, because the 256 bits are what the hashing argument below rests on.
The contract's first draft wrote 68; amendment `A-C-K1-2` corrected it to 69.
The `atom_` prefix is what distinguishes the credential from a JWT — a JWT's
first segment is base64url-encoded JSON, which never begins with `atom_` — and it
is how the authenticated extractor routes the value. It is presented in the same
header as an access token, `Authorization: Bearer atom_…`; there is no second
header and no query-parameter form.

**Ownership.** The key belongs to exactly one user. `api_keys.user_id`
references `users(id) ON DELETE CASCADE`, and every statement that reads or
writes a key carries that `user_id` as a bound parameter, under the same rule as
every other private resource
([`ADR-0005-ownership-enforced-in-sql.md`](ADR-0005-ownership-enforced-in-sql.md)).
A key id that belongs to another account is `404`, never `403`. A presented key
resolves to exactly one `user_id`, and that id is the whole identity the handler
sees — the key carries no second identity, no tenant and no service account.

**Authority.** A key has **exactly the authority of its owner**, and no more.
This is deliberately not a permission system: there is no scope table, no
per-operation grant and no read-only key. "Scoped" in this ADR means *scoped to
one owner*, not *scoped to a subset of that owner's operations*.

**Storage.** Only a **SHA-256** digest is stored, as 64 lower-case hex characters
in `api_keys.key_hash`, which is `UNIQUE`. The row also stores a `prefix`
(`atom_` plus the first eight hex characters), so a key can be shown in a
listing and recognised by its owner without the secret being recoverable. The
full secret exists in exactly two places: the `201` response that creates it, and
whatever the caller stores it in.

**Revocation.** `api_keys.revoked_at` is set by `DELETE /api/v1/api-keys/{key_id}`,
which answers `204`. A revoked key is `401` on the next request. Deleting the
owning account deletes its keys through the foreign key's `ON DELETE CASCADE`.

**The three operations**, under `API_PREFIX` (`/api/v1`), each authenticated by
an existing credential — a user access token or another API key:

| Method | Path | Success |
| ------ | ---- | ------- |
| `POST` | `/api-keys` | `201`, the only response that carries the secret |
| `GET` | `/api-keys` | `200`, a page of keys, never a secret |
| `DELETE` | `/api-keys/{key_id}` | `204` |

**Usage stamping.** `last_used_at` is written **at most once per hour per key**.
The stamp answers "was this key used recently", not "was it used at 14:03", and
the bound is what stops a read-only integration from turning every request into
an `UPDATE` on the table.

**Failure modes.** A revoked key is `401`. A malformed value, an unknown value,
and a value with the right prefix but the wrong shape are all `401`, never `500`
— classifying a credential must not be able to reach an internal error. An
**inactive account's key is `403`** with `messages.auth.account_disabled`, which
is the answer a valid JWT of the same account already receives from
`AuthService::authenticate_bearer`. The line is between the two questions, not
between the two credential forms: `401` means the credential itself is not
accepted, and `403` means the credential is valid and its account may not act.
The chain contract's `A-C-K1-1` amendment fixed this before any code existed,
because its first draft gave a disabled account `401` on the key path and `403`
on the token path. The full table is in
[`../architecture/security-architecture.md`](../architecture/security-architecture.md)
§2.2.

When this was recorded the feature was `In progress`: the migration, the
`src/api_keys/` module and the three routes were in the tree, and nothing about
the feature was `Verified`.

### Why SHA-256 and not Argon2 — and why this is not an oversight

The password rule in this repository is Argon2id (`src/auth/password.rs`), and it
stays Argon2id. **An API key is deliberately hashed differently, and that
difference must not be "corrected" in a later review.**

- A **password** is chosen by a human. It is drawn from a small, guessable space,
  it is reused across services, and it appears in dictionaries. A fast hash of a
  password is therefore an offline guessing target, and the memory-hard cost of
  Argon2id is what makes that guessing expensive.
- An **API key** is 256 bits of operating-system CSPRNG output. There is no
  dictionary, no pattern, no reuse across services and no human choice. Against a
  disclosed digest the only attack is to guess 256 random bits, which is not an
  attack. A memory-hard function therefore buys **nothing** against the value it
  protects.
- It would cost a great deal, though. Argon2id at this project's password
  parameters — 19 MiB of memory by default (`auth.argon2.memory_kib = 19456` in
  `config/auth.toml`), with an 8 MiB floor — takes tens of milliseconds and that
  memory on **every authenticated request**. Any caller could send a malformed
  `atom_…` value and make the process spend that cost before rejecting it. That
  is a self-inflicted denial of service, traded for no security at all.

So: **passwords are memory-hard hashed because they are guessable; 256-bit
random API keys are fast hashed because they are not.** The comparison is an
equality lookup on the digest, the digest is never logged and never returned, and
the plaintext is never persisted. The same reasoning is written out at the top of
`migrations/0007_api_keys.sql` and in `src/api_keys/secret.rs`, because the
column is where a future change would be made.

## Alternatives considered

| Alternative | Why it was not chosen |
| ----------- | --------------------- |
| A long-lived JWT (for example a one-year access token) | Nothing can revoke it. The token is verified statelessly, there is no key identifier and no revocation list, so a leaked token is valid until it expires — and this repository already records that the signing secret has no rotation procedure either ([`../architecture/security-architecture.md`](../architecture/security-architecture.md) §4). Adding a blocklist would make verification stateful and re-introduce the database lookup the design avoids, while adding a second revocation mechanism to maintain. |
| OAuth 2.0 client credentials | It requires an authorization server, which ATOM does not have and which would be a second service to deploy, secure, monitor and back up — for one personal workspace with one machine client. It also models a *client* identity distinct from the user, which this product does not have and does not need. |
| Reusing the refresh token as the workflow credential | This is the failure mode that motivated the decision. Rotation revokes the presented token, and reuse detection revokes **every** session of the account. A retried request or a second workflow branch locks the owner out. |
| Mutual TLS | Operationally heavy for one personal workspace: a private CA, certificate issuance and renewal, a proxy that terminates TLS and forwards the verified identity, and a renewal failure that breaks the workflow silently. The API does not terminate TLS at all today, so it would also require the deployment to change before the feature could exist. |
| A shared static secret in the environment | It is not per-user, so it cannot be attributed to an account and cannot express "this workflow acts as this user"; it is not revocable per client, only by redeploying every instance; and an audit cannot distinguish two integrations using it. It also breaks the rule that no file in `config/` holds a secret and that an environment variable is for a genuinely deployment-wide value. |
| Scoped (read-only or per-operation) keys | Not rejected as a bad idea — rejected as **not this decision**. It needs a scope model, a table and a per-request authorisation check that no requirement asks for yet. Its absence is recorded as a negative consequence below, not hidden. |

## Consequences and trade-offs

### Positive

- **The integration becomes possible without a human.** A workflow holds one
  credential, presents it on every run, and rotates nothing. No refresh cycle, so
  no reuse detection, so no path by which the automation locks the owner out.
- **Keys are revocable, and independently so.** Two workflows can hold two keys;
  revoking one leaves the other and the owner's sessions untouched. This is
  strictly better than the signing secret, which cannot be retired at all
  without restarting every instance.
- **Keys are attributable.** `GET /api/v1/api-keys` lists `name`, `prefix`,
  `created_at`, `last_used_at` and `revoked_at` per key, so the owner can see
  which integration holds which credential and whether it is still in use.
- **A database disclosure does not yield a usable key**, exactly as for refresh
  tokens: only the SHA-256 digest is stored.
- **The token design is unchanged.** Access and refresh tokens keep their
  lifetime rules, their rotation and their reuse detection; nothing about the
  human login flow is relaxed to accommodate a machine.

### Negative

- **A key is a bearer secret with its owner's full authority.** Anyone who reads
  it can do anything the owner can do — read every note, claim and application,
  and delete them. It has to be handled like a password in transit and at rest by
  whoever stores it: never committed to a repository, never written into a log,
  never placed in a system whose exposure is wider than the API's own.
- **There is no scope limiting.** A read-only key, an expiring key or a key
  restricted to `/api/v1/notes` cannot be issued. Any integration that only needs
  to create tasks still gets the whole account.
- **A key can mint another key.** `POST /api/v1/api-keys` accepts an API key as
  the credential, so a leaked key can create a replacement that outlives the
  revocation of the leaked one. Revocation is therefore not sufficient on its own
  once a key is known to be compromised: the owner must revoke **every** key of
  that account, and must also consider that the workflow's stored copy is still
  valid until it is replaced.
- **There is no expiry.** A key lives until it is revoked. Nothing in the design
  forces a periodic re-issue, so an unused-but-unrevoked key is a permanent
  credential.
- **`last_used_at` is coarse.** It is updated at most once per hour per key, so
  it answers "was this key used recently", not "was it used at 14:03".
- **The plaintext lives outside ATOM's control.** Once returned, the secret is in
  the caller's credential store. ATOM cannot audit where it was copied.
- **The authentication path gains a second branch.** Every failure mode has to be
  enumerated and tested twice, and a mistake in the routing of a value between
  the two paths is a new class of defect.

## Security and operational implications

**The secret is returned exactly once.** `POST /api/v1/api-keys` is the only
response that contains it. `GET /api/v1/api-keys` returns `prefix`, never the
secret; the secret is never logged, never included in an error message and never
stored in plaintext. This is the same rule as the refresh token and the password.

**The prefix is what makes a key identifiable without being usable.** `atom_`
plus eight hex characters is 32 bits of the secret; the other 224 bits are
absent. It is not unique and must not be made unique: two keys of one owner can
share a prefix legitimately, and `key_hash` is the identity a lookup seeks on.

**`last_used_at` is written at most once per hour per key.** The stamp exists so
an owner can find a key that is no longer used, and a write on every request
would turn a read-only integration into a continuous `UPDATE` on the table. The
bound is the trade: visibility is coarse and deliberately so.

**The failure modes answer two different questions.** A revoked key, an unknown
key and a malformed key are `401`: the credential itself is not accepted. An
inactive account's key is `403` with `messages.auth.account_disabled`: the
credential is valid and the account may not act. That is what a JWT of the same
account already receives. The second answer discloses that the account exists and
is disabled; keeping the two credential forms consistent is worth more than
withholding it from the key path only, and the alternative — `401` for a key and
`403` for a token of the same account — would be a contradiction inside the
authentication boundary.

**The digest is a secret-adjacent value.** It is stored with a `UNIQUE`
constraint and never leaves the database. Unlike a password hash it is not
guessed offline, because its input has 256 bits of entropy; the reasoning is in
the decision section and is the reason this control is allowed to differ from the
password rule.

**A deployment must not hand a key to a client that cannot keep it.** The key is
a long-lived, full-authority bearer secret. It belongs in a server-side
credential store — the n8n credential store, a secret manager, an environment
variable of a service the owner controls — and **not** in a browser bundle, a
mobile app, a client-side script or a URL. The API enforces none of this; it is
an operator obligation, and it is the reason the feature is aimed at a workflow
rather than at an end-user client. A key that may have leaked is **revoked**, not
"monitored": the control is one row update, and watching it is not a substitute
for retiring it.

**Nothing restarts to revoke a key.** Revocation is effective on the next
request, so an exposed key can be retired without touching the signing secret,
the process or any other session.

**Not rate limited separately.** The per-process limiter covers `register`,
`login` and `refresh` only. An API key cannot be brute forced (256 bits), but an
automation shares the deployment's request budget with its owner, and a runaway
workflow shows up as load rather than as a `429`.

## Related requirements and changes

| Reference | Relationship |
| --------- | ------------ |
| [`../project-truth.md`](../project-truth.md) §9 item 9 | The requirement this serves: an automation or AI integration must not weaken ownership checks, token handling or secret hygiene. |
| [`../project-truth.md`](../project-truth.md) §4.2 | The authentication requirements. They list no machine credential today, so this ADR is the record of that requirement rather than a restatement of an existing row. |
| [`ADR-0003-auth-token-design.md`](ADR-0003-auth-token-design.md) | The token design whose rotation and reuse detection a machine client cannot use. |
| [`ADR-0005-ownership-enforced-in-sql.md`](ADR-0005-ownership-enforced-in-sql.md) | The ownership rule a key inherits, unchanged. |
| [`../architecture/security-architecture.md`](../architecture/security-architecture.md) §2 | The authentication boundary this adds a second credential form to. |
| [`../api/README.md`](../api/README.md) §3 | The published surface, which moves from 51 to 54 operations. |
| [`../roadmap.md`](../roadmap.md) | The n8n integration. It stays `Proposed`; this ADR decides the credential, not the integration, which remains outside the agreed scope. |
| [`../engineering/runs/archive/phase-2/ledger.md`](../engineering/runs/archive/phase-2/ledger.md) `C-K1` | The locked contract that specifies the schema, the types, the invariants and the test names for the implementation, including the `A-C-K1-1` amendment on the inactive-account answer. |
| [`../engineering/change-log.md`](../engineering/change-log.md) | The append-only record; the entry for this change is appended with the chain. |

## Supersedes / Superseded by

Supersedes: none.

Superseded by: none. A review of this decision is due when the requirement it
serves changes.
