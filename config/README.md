# ATOM configuration

This directory is the **single source of truth** for the application's
non-secret settings. It is read once, at start-up, and the result is validated
before the server binds a port.

- The loader is `src/config/`: `mod.rs` holds `Config` and `Environment`,
  `loader.rs` discovers and merges the files, `settings.rs` holds the typed
  structures, `validation.rs` holds the rules and `messages.rs` holds the
  client-facing message catalogue. The module's own tests are in
  `src/config/tests.rs`.
- The operator-facing reference is
  [`docs/architecture/deployment.md`](../docs/architecture/deployment.md).
- The environment-variable template is [`.env.example`](../.env.example).

---

## 1. Layout

```text
config/
├── app.toml                    the server, logging and request limits
├── api.toml                    the API metadata, CORS, pagination, rate limit
├── auth.toml                   the token lifetimes, the Argon2 cost, the password policy
├── database.toml               the connection pool and the timeouts
├── messages.toml               the client-facing message catalogue
└── environments/
    ├── development.toml        the overrides for a local machine
    └── production.toml         the overrides for a production deployment
```

Every file here is committed and safe to read. **No file in this directory may
contain a secret.** The signing secret and the database password come from the
environment; see §4.

## 2. Precedence

Three layers, later wins over earlier, key by key:

```text
1. the base files in config/          app, api, auth, database, messages
2. the environment overlay            config/environments/<APP_ENV>.toml
3. the environment variables          the names in .env.example
```

The environment is selected by `APP_ENV`, which accepts `development` (`dev`,
`local`) or `production` (`prod`). It defaults to `development`.

A value that is not set in a layer keeps the value from the layer below it.
There is no hidden fourth layer: a setting that is not in a file and not in an
environment variable uses the default compiled into `src/config/settings.rs`.

### Where the files come from

The files are **embedded in the binary**, so a deployment needs no
configuration directory. The loader looks for a directory in this order:

1. the path in `ATOM_CONFIG_DIR`, if it is set;
2. a `config` directory next to the executable;
3. a `config` directory in the working directory.

A file found on disk is merged **on top of** the embedded copy, key by key.
An operator who wants to change one setting can therefore write a file that
contains only that setting. A file that is not on disk leaves the embedded
copy in place.

`ATOM_CONFIG_DIR` that points at a directory which does not exist is an
error: the operator asked for it explicitly, so a silent fallback would hide
the mistake.

## 3. The settings

The table lists every setting. "Environment variable" is the name that
overrides the file value; a setting with no variable can only be changed in a
file.

### `app.toml`

| Key | Default | Environment variable | Meaning |
| --- | ------- | -------------------- | ------- |
| `app.name` | `ATOM API` | — | The product name, used in the OpenAPI document and the start-up log. |
| `app.bind_addr` | `127.0.0.1:3000` | `APP_BIND_ADDR` | The socket the server binds to. Loopback by default, so a deployment that wants the API reachable has to say so. |
| `app.log_filter` | `info` | `APP_LOG_FILTER` | The `tracing` filter directive. An invalid filter is a start-up error. |
| `app.limits.max_body_bytes` | `1048576` | `APP_MAX_BODY_BYTES` | The largest accepted request body. |
| `app.limits.request_timeout_seconds` | `30` | `APP_REQUEST_TIMEOUT_SECONDS` | How long a request may take before it is abandoned with a `408`. |
| `app.limits.readiness_probe_timeout_seconds` | `1` | `APP_READINESS_PROBE_TIMEOUT_SECONDS` | How long the readiness probe waits for the database. |
| `app.sessions.prune_interval_seconds` | `3600` | `APP_SESSION_PRUNE_INTERVAL_SECONDS` | How often expired refresh sessions are pruned. |
| `app.sessions.prune_grace_seconds` | `86400` | `APP_SESSION_PRUNE_GRACE_SECONDS` | How long an expired session is kept. **This is a security window, not a tuning knob** — a revoked session has to survive long enough that a replayed token is still recognised as reuse (ADR-0003). |

### `api.toml`

| Key | Default | Environment variable | Meaning |
| --- | ------- | -------------------- | ------- |
| `api.title` | `ATOM API` | — | The title of the OpenAPI document. |
| `api.docs_enabled` | `true` | `APP_API_DOCS_ENABLED` | Whether the OpenAPI document and the API explorer are served. The production overlay turns them off. |
| `api.cors.allowed_origins` | `[]` | `APP_CORS_ALLOWED_ORIGINS` | The origins allowed to make cross-origin requests. Empty disables CORS. A wildcard is refused at start-up. |
| `api.pagination.default_limit` | `20` | — | The page size used when a caller does not ask for one. **Part of the published client convention**: changing it changes what a client receives. |
| `api.pagination.max_limit` | `100` | — | The largest page a caller may ask for. Also part of the published client convention. |
| `api.rate_limit.max_requests` | `10` | `APP_AUTH_RATE_LIMIT_MAX_REQUESTS` | How many authentication attempts one client may make per window. |
| `api.rate_limit.window_seconds` | `60` | `APP_AUTH_RATE_LIMIT_WINDOW_SECONDS` | The length of the rate-limit window. |

### `auth.toml`

| Key | Default | Environment variable | Meaning |
| --- | ------- | -------------------- | ------- |
| `auth.access_token_ttl_seconds` | `900` | `APP_ACCESS_TOKEN_TTL_SECONDS` | The lifetime of an access token. |
| `auth.refresh_token_ttl_seconds` | `1209600` | `APP_REFRESH_TOKEN_TTL_SECONDS` | The absolute lifetime of a refresh token. Must be longer than the access-token lifetime. |
| `auth.issuer` | `atom-api` | `APP_JWT_ISSUER` | The `iss` claim, written into every token and required back from it. |
| `auth.audience` | `atom-api` | `APP_JWT_AUDIENCE` | The `aud` claim. |
| `auth.clock_skew_seconds` | `30` | `APP_ACCESS_TOKEN_CLOCK_SKEW_SECONDS` | The clock-skew allowance when validating an access token. A deployment whose hosts drift by more than this sees unexplained `401` responses. |
| `auth.argon2.memory_kib` | `19456` | `APP_ARGON2_MEMORY_KIB` | The Argon2id memory cost. It may be raised; it may not be lowered below 8192 KiB, which is a security floor rather than a setting. |
| `auth.argon2.iterations` | `2` | `APP_ARGON2_ITERATIONS` | The Argon2id iteration count. |
| `auth.argon2.parallelism` | `1` | `APP_ARGON2_PARALLELISM` | The Argon2id degree of parallelism. |
| `auth.password.min_length` | `12` | — | The shortest accepted password. |
| `auth.password.max_length` | `256` | — | The longest accepted password. The maximum bounds the cost of hashing a very long password. |

### `database.toml`

| Key | Default | Environment variable | Meaning |
| --- | ------- | -------------------- | ------- |
| `database.max_connections` | `10` | `DATABASE_MAX_CONNECTIONS` | The size of the connection pool. |
| `database.acquire_timeout_seconds` | `5` | `DATABASE_ACQUIRE_TIMEOUT_SECS` | How long a caller waits for a pooled connection. Zero would mean "wait forever", so it is refused. |
| `database.connect_timeout_seconds` | `10` | `DATABASE_CONNECT_TIMEOUT_SECS` | How long the initial connection may take. Without a bound, an unreachable database hangs the process before it binds a port. |

### `messages.toml`

The client-facing message catalogue. See §5.

## 4. The secrets

Two values are secrets and have **no file layer at all**:

| Variable | What it is |
| -------- | ---------- |
| `DATABASE_URL` | The PostgreSQL connection URL. It usually contains a password. |
| `APP_JWT_SECRET` | The HMAC signing secret. At least 32 bytes; the documented placeholder is refused when `APP_ENV` is `production`. |

Both are **required**. A missing or empty value stops the process before it
connects to anything. The error reports the length of a rejected secret, never
its content, and the types that carry a secret print `[REDACTED]` in their
`Debug` output.

A file in this directory may therefore never contain a secret. If a setting
needs a credential, it is an environment variable.

## 5. The message catalogue

`messages.toml` holds every string that reaches a client through the error
envelope. The **stable error code** is not in this file: the code is part of
the API contract and lives in `src/error.rs`, so that a client can switch on
it without the text ever being able to change it.

A template may contain `{}` placeholders. The values substituted into them
are always numbers the application controls, never user input, so there is no
injection risk. A placeholder without a matching argument is left as `{}`
rather than silently dropped, so a mistake stays visible.

The two messages for a database failure and for an unexpected failure are
**not** in the catalogue: they are part of the error type, because they must
never be replaced by something that reveals an internal detail.

## 6. Validation

The resolved configuration is validated once, before the socket is bound. The
rules are in `src/config/validation.rs`. A configuration that violates one of
them is an error, not a warning:

- the two secrets are present, and the signing secret is at least 32 bytes;
- in `production`, the documented placeholder secret is refused;
- the request timeout, the readiness timeout and the prune interval are
  positive, and the prune grace is positive;
- the body limit is positive;
- the pagination default is positive and the maximum is at least the default;
- the rate limit is positive and its window is not zero;
- the CORS allowlist contains no wildcard and every origin is a valid header
  value;
- the pool size is positive and neither the acquire nor the connect timeout is
  zero;
- the access-token lifetime is positive and the refresh-token lifetime is
  strictly longer;
- the issuer and the audience are not empty;
- the Argon2 memory is at least 8192 KiB, and the iteration count and the
  degree of parallelism are at least one;
- the password policy has a positive minimum and a maximum that is at least
  the minimum.

## 7. Adding a setting

1. **Search first.** The setting may already exist, in a file, in an
   environment variable, or as a constant in the code.
2. Decide where it belongs:
   - a value a deployment may need to change at deploy time gets an
     environment variable as well;
   - a value the **database** enforces stays a constant in the code, next to
     the constraint it mirrors, because a migration is the enforced copy and
     cannot read a file;
   - a value that is part of the **published client contract** — the
     pagination limits — stays in a file, and changing it is a breaking
     change.
3. Add the key to the file, the field to `src/config/settings.rs`, the
   environment override to `src/config/loader.rs` and the validation rule to
   `src/config/validation.rs`.
4. Add the setting to `.env.example` if it has an environment variable, and
   to the reference table in `docs/architecture/deployment.md`.
5. Add a test in `src/config/tests.rs`: the default, the override, and the
   validation rule.
6. Record the change in `docs/engineering/change-log.md`.
