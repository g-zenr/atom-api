-- ATOM Phase 2 — scoped API keys for machine clients.
--
-- One row per long-lived credential an unattended client can hold. Phase 1 has
-- no credential a workflow can survive: the access token expires after fifteen
-- minutes and the refresh token rotates with reuse detection, so a scheduled
-- job that stores one either stops working when it rotates or trips the reuse
-- alarm and revokes the account's sessions. A key is the credential for a
-- machine, and it is still scoped to exactly one owner: every statement that
-- reads or writes it carries `user_id` in its own `WHERE` clause, like every
-- other user table (ADR-0005).
--
-- The definition of every column, constraint and index here is
-- docs/database/schema.md §11.7. This file is the enforced copy of that
-- description; if the two ever disagree, the document is corrected in a new
-- change and the migration stays as it was applied.
--
-- ---------------------------------------------------------------------------
-- WHAT IS STORED. READ THIS BEFORE CHANGING `key_hash`.
-- ---------------------------------------------------------------------------
--
-- Only the SHA-256 hex digest of the secret is persisted, for the same reason
-- `0001` persists only the digest of a refresh token: a database leak yields
-- rows, not usable credentials. The secret itself is returned once, in the
-- create response, and is never stored, logged or returned again.
--
-- **`key_hash` is SHA-256 on purpose, and it is not a mistake to be "fixed"
-- into Argon2 later.** The two credentials this database hashes have different
-- threat models, and the difference decides the algorithm:
--
--   * A password is low-entropy and chosen by a person. An attacker holding its
--     digest can guess candidates, so the only defence left is to make each
--     guess expensive. That is what `users.password_hash` is for, and Argon2 is
--     correct there.
--   * A key is not chosen by anyone. It is 32 bytes from the operating system's
--     CSPRNG, rendered as `atom_` plus 64 lower-case hex characters — 69
--     characters carrying 256 bits of entropy. There is no dictionary, no
--     pattern and nothing to guess, so a memory-hard function buys nothing
--     here.
--   * The digest is verified on **every authenticated request**. Argon2id at
--     this project's password parameters (`config/auth.toml`: 19 MiB, two
--     iterations, with an 8 MiB floor) deliberately costs tens of milliseconds
--     and most of 20 MiB per verification. On the hot path that turns a
--     leaked-database risk this table already survives into unauthenticated CPU
--     and memory amplification against our own API — a self-inflicted denial of
--     service, with the attacker supplying nothing but a header.
--
-- So: SHA-256 for a key, Argon2 for a password, and the asymmetry is the
-- design, not an oversight. It is recorded in ADR-0011. Changing this column to
-- a memory-hard hash needs a new ADR and a measured argument, not a consistency
-- edit made to match `users.password_hash`.
--
-- ---------------------------------------------------------------------------
-- WHY `prefix` EXISTS, AND WHY IT IS NOT UNIQUE.
-- ---------------------------------------------------------------------------
--
-- `prefix` is `atom_` plus the first 8 hex characters of the secret. It is
-- stored so that an operator can identify a key in a listing, a screenshot or a
-- support ticket without the secret ever being recoverable: the other 56 hex
-- characters are stored nowhere at all, so 32 bits is the whole of what a
-- reader of this table gets, which is not enough to authenticate. It is a
-- label: not the identity, and not a lookup key. It must **not** be made
-- unique — two keys of one owner can legitimately share eight hex characters,
-- a collision leaks nothing and disambiguates nothing, and rejecting a legal
-- key for it would be a defect. `key_hash` is the identity, and authentication
-- seeks on the digest, never on the prefix.
--
-- The CHECKs below pin the *form* of each value, so a row the issuing code
-- could never have produced — a prefix outside `atom_` + 8 lower-case hex, a
-- digest that is not 64 lower-case hex characters — is unrepresentable whatever
-- writes it. That matters beyond tidiness: the middleware decides "is this
-- bearer token a JWT or an API key?" by the `atom_` prefix, so a stored prefix
-- outside that grammar would be a credential the middleware could not classify.
--
-- Revocation is a timestamp, not a status column: a key is never un-revoked, so
-- `revoked_at IS NULL` *is* "active" and there is no second representation of
-- the same fact to drift from it. `last_used_at` is nullable for the same
-- reason: it records the last authenticated use, not a state, and the
-- once-an-hour write throttle is a service rule because a row CHECK cannot see
-- the previous value. There is no `updated_at`: the row is immutable apart from
-- those two stamps, and a column the hourly touch would rewrite carries no
-- information the touch itself does not.
--
-- Deleting the owning account deletes its keys. A key is an authority derived
-- from the account and has no meaning without it, so `CASCADE` is the only
-- correct behaviour; there is no child table and therefore no composite
-- ownership key to enforce.

CREATE TABLE api_keys (
    id           UUID        PRIMARY KEY,
    user_id      UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- Owner-chosen label, so a person can tell their own keys apart.
    name         TEXT        NOT NULL,
    -- Display half of the secret: `atom_` + 8 hex. A label, not an identity.
    prefix       TEXT        NOT NULL,
    -- SHA-256 hex digest of the whole secret. SHA-256, not Argon2: see above.
    key_hash     TEXT        NOT NULL,
    -- Last authenticated use, written at most once an hour per key.
    last_used_at TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- NULL means active. Set once; never cleared.
    revoked_at   TIMESTAMPTZ,
    CONSTRAINT api_keys_name_length_chk CHECK (CHAR_LENGTH(name) BETWEEN 1 AND 100),
    CONSTRAINT api_keys_prefix_chk      CHECK (prefix ~ '^atom_[0-9a-f]{8}$'),
    CONSTRAINT api_keys_key_hash_chk    CHECK (key_hash ~ '^[0-9a-f]{64}$'),
    -- "One row per secret". Named rather than left to PostgreSQL's default
    -- naming, because the credential lookup in `src/api_keys/repository.rs`
    -- documents its access path by this name, and because an unnamed column
    -- constraint would rename itself if the column list ever changed. A second
    -- row holding the same digest is one credential stored twice: the insert
    -- must fail with `23505`, not leave the outcome of a lookup dependent on
    -- which row the planner finds. A secret is 256 bits of CSPRNG output, so a
    -- collision between two independently generated keys is a defect, not an
    -- "unlikely" case to tolerate.
    CONSTRAINT api_keys_key_hash_key    UNIQUE (key_hash)
);

-- The owner's listing (`GET /api/v1/api-keys`, newest first) and the owner
-- predicate of the revoke, delete and usage-stamp statements, which all carry
-- `user_id`. The digest's index cannot serve them: a caller that sends only its
-- own id has no digest to seek on. It is also the index `ON DELETE CASCADE`
-- uses to find a departing account's keys, without which deleting a user scans
-- this table.
--
-- One column, not `(user_id, created_at DESC)`: a person holds a handful of
-- keys, so sorting that owner's few rows after the index seek costs nothing,
-- while a second column would be written for every row and serve no read.
CREATE INDEX api_keys_user_id_idx ON api_keys (user_id);
