# ATOM — deployment

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-26
> **Owner:** maintained by the `architect` agent

How the ATOM API is configured, started, observed and run in production —
and what is still missing before it can be.

---

## 1. Configuration

The configuration is resolved **once**, at start-up, by the `config` module in
`src/config/`, and then handed to the rest of the application as a typed
`Config` value. No module reads the environment or a configuration file
itself.

The configuration reference for the files themselves is
[`../../config/README.md`](../../config/README.md). This section is the
operator's contract: what a deployment may change, how, and what stops the
process.

### 1.1 The layers and the precedence

Three layers, later wins over earlier, key by key:

```text
1. the base files in config/          app, api, auth, database, ml, messages
2. the environment overlay            config/environments/<APP_ENV>.toml
3. the environment variables          the names in .env.example
```

A value that is **not** set in a layer keeps the value from the layer below
it. There is no hidden fourth layer: a setting that is in no file and in no
environment variable uses the default compiled into `src/config/settings.rs`.

The overlay is selected by `APP_ENV`, which accepts `development` (`dev`,
`local`) or `production` (`prod`) and defaults to `development`. Any other
value stops the process.

The two secrets have **no file layer at all**: `DATABASE_URL` and
`APP_JWT_SECRET` are required environment variables, and no file in `config/`
may contain a secret. See §2.

**The environment layer wins over the files.** A variable that is set — even
to a value that the committed file does not use — replaces the file value.
`.env.example` is a template that ships a working value for every setting, so
a `.env` copied from it is an **override** of the committed files, not a
neutral copy of them. The setting where that matters most is
`APP_API_DOCS_ENABLED`, which the production overlay turns off and the
template turns back on. See §5.2.

### 1.2 Where the files come from

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

### 1.3 The settings

The tables below list every setting. "Environment variable" is the name that
overrides the file value; a setting with no variable can only be changed in a
file.

#### `app.toml`

| Key | Default | Environment variable | Constraint |
| --- | ------- | -------------------- | ---------- |
| `app.bind_addr` | `127.0.0.1:3000` | `APP_BIND_ADDR` | Must be a valid socket address. |
| `app.log_filter` | `info` | `APP_LOG_FILTER` | Must parse as a `tracing` filter. An unparsable filter is a start-up error, not a silent fallback to `info`. |
| `app.limits.max_body_bytes` | `1048576` (1 MiB) | `APP_MAX_BODY_BYTES` | Must be positive and at most 4 MiB, the ceiling of the body reader. It is the ceiling the router applies to a request body; the extractor applies an independent 4 MiB backstop, and a value above it is refused at start-up. See §4. |
| `app.limits.request_timeout_seconds` | `30` | `APP_REQUEST_TIMEOUT_SECONDS` | Must be positive. A request that exceeds it is abandoned with a `408`. |
| `app.limits.readiness_probe_timeout_seconds` | `1` | `APP_READINESS_PROBE_TIMEOUT_SECONDS` | Must be positive. It bounds the readiness probe. |
| `app.limits.shutdown_grace_seconds` | `10` | `APP_SHUTDOWN_GRACE_SECONDS` | Must be positive. The listener stops accepting as soon as the signal arrives, then drains; this value is the **ceiling** on that drain, so a request that never completes cannot keep the process alive. It must stay below the time the orchestrator allows the process to stop. |
| `app.sessions.prune_interval_seconds` | `3600` | `APP_SESSION_PRUNE_INTERVAL_SECONDS` | Must be positive. |
| `app.sessions.prune_grace_seconds` | `86400` | `APP_SESSION_PRUNE_GRACE_SECONDS` | Must be positive **and at least `3600` (one hour)**. The grace keeps the row of an expired session alive for a while, so that a token that expired recently is still recognised as *expired* rather than as one that never existed. It may be raised, never lowered below the floor. See *The floors that are not settings* below. |

#### `api.toml`

| Key | Default | Environment variable | Constraint |
| --- | ------- | -------------------- | ---------- |
| `api.title` | `ATOM API` | — | The title of the OpenAPI document. |
| `api.docs_enabled` | `true` | `APP_API_DOCS_ENABLED` | A boolean: `true`, `false`, `1`, `0`, `yes`, `no`, `on` or `off`. The production overlay turns the document and the explorer off. A `--no-default-features` build cannot serve them at all, whatever this value says; see 5.1. |
| `api.cors.allowed_origins` | `[]` | `APP_CORS_ALLOWED_ORIGINS` | A comma-separated list. Empty disables CORS. A wildcard is refused, and every entry must be a valid header value. The variable overrides the file, so a non-empty value in a `.env` enables CORS; see §5.2. |
| `api.pagination.default_limit` | `20` | — | Must be positive. **Part of the published client convention**: changing it changes what a client receives, so it is a breaking change for a client. |
| `api.pagination.max_limit` | `100` | — | Must be at least the default limit and at most 1000 items. Also part of the published client convention, and therefore a breaking change for a client. |
| `api.rate_limit.max_requests` | `10` | `APP_AUTH_RATE_LIMIT_MAX_REQUESTS` | Must be positive. |
| `api.rate_limit.window_seconds` | `60` | `APP_AUTH_RATE_LIMIT_WINDOW_SECONDS` | Must be positive. |

#### `auth.toml`

| Key | Default | Environment variable | Constraint |
| --- | ------- | -------------------- | ---------- |
| `auth.access_token_ttl_seconds` | `900` (15 min) | `APP_ACCESS_TOKEN_TTL_SECONDS` | Must be positive and at most `86400` (one day). |
| `auth.refresh_token_ttl_seconds` | `1209600` (14 days) | `APP_REFRESH_TOKEN_TTL_SECONDS` | Must be **longer** than the access-token lifetime and at most `31536000` (one year). |
| `auth.issuer` | `atom-api` | `APP_JWT_ISSUER` | Must not be empty. It is the `iss` claim, written into every token and required back from it. The key in `config/auth.toml` is `auth.issuer`: the `jwt_` prefix is not part of it. |
| `auth.audience` | `atom-api` | `APP_JWT_AUDIENCE` | Must not be empty. It is the `aud` claim. The key in `config/auth.toml` is `auth.audience`: the `jwt_` prefix is not part of it. |
| `auth.clock_skew_seconds` | `30` | `APP_ACCESS_TOKEN_CLOCK_SKEW_SECONDS` | The clock-skew allowance applied when validating an access token. Must be positive and at most `300` (five minutes). A deployment whose hosts drift by more than this sees unexplained `401` responses. See *The floors that are not settings* below. |
| `auth.argon2.memory_kib` | `19456` (19 MiB) | `APP_ARGON2_MEMORY_KIB` | Must be at least `8192` (8 MiB), which is a security floor rather than a setting. |
| `auth.argon2.iterations` | `2` | `APP_ARGON2_ITERATIONS` | Must be at least one. |
| `auth.argon2.parallelism` | `1` | `APP_ARGON2_PARALLELISM` | Must be at least one. |
| `auth.password.min_length` | `12` | — | Must be positive, not above the maximum, and at least `12` (the policy floor). See *The floors that are not settings* below. |
| `auth.password.max_length` | `256` | — | Must be at least the minimum. The maximum bounds the cost of hashing a very long password. |

#### `database.toml`

| Key | Default | Environment variable | Constraint |
| --- | ------- | -------------------- | ---------- |
| `database.max_connections` | `10` | `DATABASE_MAX_CONNECTIONS` | Must be positive. |
| `database.acquire_timeout_seconds` | `5` | `DATABASE_ACQUIRE_TIMEOUT_SECS` | Must not be zero: a zero value would mean "wait forever". |
| `database.connect_timeout_seconds` | `10` | `DATABASE_CONNECT_TIMEOUT_SECS` | Must not be zero. Without a bound, an unreachable database hangs the process before it binds a port. |

One **server-side** setting belongs to this table's subject but not to this
table: PostgreSQL's `work_mem` decides whether the semantic search's sort spills,
and the same statement costs 14–16 ms or 75–123 ms depending on it. The API
cannot read or set it; it is the operator's, and it is §7.1.

#### The floors that are not settings

Five settings have a bound that a deployment may not cross. The bound is a
constant in `src/config/validation.rs`, not a setting: a configuration that
crosses it is a start-up error.

| Setting | The bound | Why it is a floor and not a knob |
| ------- | --------- | -------------------------------- |
| `auth.password.min_length` | At least `12` characters. | A deployment may **raise** the minimum, never lower it below the floor. The floor is what keeps the guessing cost of a leaked credential list at a level the product is willing to stand behind. |
| `auth.clock_skew_seconds` | At most `300` seconds. | A deployment may **lower** the allowance to tighten the expiry check, never raise it beyond the bound. An unbounded allowance would accept a token long after it expired. |
| `auth.access_token_ttl_seconds` | At most `86400` seconds (one day). | The access token is the short-lived half of the token design. A lifetime longer than a day is far beyond any usable session. |
| `auth.refresh_token_ttl_seconds` | At most `31536000` seconds (one year). | The refresh token is the long-lived half. The bound keeps the arithmetic of the absolute deadline well inside the range of a timestamp. |
| `app.sessions.prune_grace_seconds` | At least `3600` seconds (one hour). | The grace keeps the row of an expired session alive, so that a token that expired recently is still recognised as *expired* rather than as one that never existed. It may be lengthened, never shortened below the bound. |

The numeric values live with the rules in
[`../../src/config/validation.rs`](../../src/config/validation.rs), as the
constants `MIN_PASSWORD_LENGTH`, `MAX_CLOCK_SKEW_SECONDS`,
`MAX_ACCESS_TOKEN_TTL_SECONDS`, `MAX_REFRESH_TOKEN_TTL_SECONDS` and
`MIN_PRUNE_GRACE_SECONDS`. The table above repeats them so that an operator
can act without reading the source; the source is the enforced copy.

#### `ml.toml`

| Key | Default | Environment variable | Constraint |
| --- | ------- | -------------------- | ---------- |
| `ml.enabled` | `false` | `APP_ML_ENABLED` | A boolean. `false` selects the `disabled` embedder whatever `ml.backend` says, so a default deployment never reads a model file. |
| `ml.backend` | `flex` | `APP_ML_BACKEND` | One of `flex` or `deterministic`. Anything else is a start-up error, never a silent fallback. `deterministic` is the hash-based embedder the test suite and the seeder use; it needs no file and no network. |
| `ml.model_path` | empty — no model | `APP_ML_MODEL_PATH` | A path to a local SafeTensors file. When it is set and the file is absent, the embedder is `disabled`; **start-up still succeeds** (§1.6). |
| `ml.tokenizer_path` | empty — the `vocab.txt` beside `ml.model_path` | `APP_ML_TOKENIZER_PATH` | A path to the WordPiece vocabulary. Unset means the sibling of the weights ([`ADR-0008`](../decisions/ADR-0008-local-pure-rust-ml.md) §10). A missing vocabulary is the disabled state, not a start-up failure. |
| `ml.dimension` | `384` | `APP_ML_DIMENSION` | Must be positive. It is a property of the model rather than a tuning knob. |
| `ml.timeout_seconds` | `2` | `APP_ML_TIMEOUT_SECONDS` | Must be positive and at most `app.limits.request_timeout_seconds`: the budget is spent inside a request and may not outlast it. It bounds **one inference**; it does not bound the hybrid re-rank, which performs no inference. |
| `ml.candidate_limit` | `200` | `APP_ML_CANDIDATE_LIMIT` | Must be positive and at most `MAX_ML_CANDIDATE_LIMIT`, **1,000**. It is the **work bound** of one re-rank — the re-rank is the one part of a search no timeout covers — and it has no relation to the page size: a candidate set smaller than a page is a well-formed short page. |

The seven settings are the whole configuration surface of the local
machine-learning feature
([`../features/ml/architecture.md`](../features/ml/architecture.md) §7). None of
them is a secret: the model is a local file, and there is no model service and
therefore no credential for one.

#### `messages.toml`

`messages.toml` holds the client-facing message catalogue, not a setting.
Every string that reaches a client through the error envelope comes from it,
resolved at the call site. The **stable error code** is not in the file: the
code is part of the API contract and lives in the `code` module in
`src/error.rs`, so that a client can switch on it without the text ever being
able to change it.

#### The environment overlays

`config/environments/development.toml` — applied when `APP_ENV` is
`development`, `dev` or `local`:

| Key | Value |
| --- | ----- |
| `app.log_filter` | `atom_api=debug,tower_http=debug,sqlx=warn` |
| `app.limits.request_timeout_seconds` | `60` |

`config/environments/production.toml` — applied when `APP_ENV` is `production`
or `prod`:

| Key | Value |
| --- | ----- |
| `app.log_filter` | `info,atom_api=info` |
| `app.limits.request_timeout_seconds` | `30` |
| `app.sessions.prune_interval_seconds` | `900` |
| `api.docs_enabled` | `false` |
| `database.max_connections` | `20` |

The overlay deliberately does **not** override the pagination bounds. They are
part of the published client convention, so a client must see the same page
size in every environment. An operator that needs a smaller page changes
`api.pagination.max_limit` in the base file, deliberately and for every
environment at once.

#### The variables that are not settings

| Variable | Meaning |
| -------- | ------- |
| `APP_ENV` | Selects the environment overlay: `development` (`dev`, `local`) or `production` (`prod`). It defaults to `development`. Any other value stops the process. |
| `ATOM_CONFIG_DIR` | The directory the configuration files are read from. It must be an existing directory. |
| `DATABASE_URL` | **Required.** The PostgreSQL connection string. It has no file layer. |
| `APP_JWT_SECRET` | **Required.** The HMAC signing secret for the access tokens. It has no file layer. At least **32 bytes**; the documented placeholder is refused when `APP_ENV` is `production`. |
| `TEST_DATABASE_URL` | The disposable database used by the integration suite. The harness refuses to run unless the database name contains `test`. |

`.env.example` holds the same variables, with a placeholder for the two
secrets and a working value for the rest. A working value is an **override**,
because the environment layer wins over the file layer; see §5.2. `.env` is
git-ignored and must never be committed.

### 1.4 The start-up contract

The process refuses to serve traffic in a partially configured state. Every
rule below stops it **before** the socket is bound.

| Rule | Where it is enforced |
| ---- | -------------------- |
| `DATABASE_URL` and `APP_JWT_SECRET` are present and not empty. | `required` in `src/config/loader.rs`; `ConfigError::Missing`. |
| `APP_ENV` names a known environment. | `Environment::parse` in `src/config/settings.rs`; `ConfigError::Invalid`. |
| `ATOM_CONFIG_DIR` points at an existing directory. | `discover_directory` in `src/config/loader.rs`; `ConfigError::Invalid`. |
| Every configuration file can be read and parsed. | `src/config/loader.rs`; `ConfigError::Io` and `ConfigError::Parse`. An unknown key is a parse error, because every settings structure uses `deny_unknown_fields`. |
| The signing secret is at least 32 bytes, and the documented placeholder is refused in production. | `validate_secret` in `src/config/validation.rs`. |
| The body limit, the request timeout, the readiness timeout, the shutdown grace, the prune interval and the prune grace are positive. | `validate_settings` in `src/config/validation.rs`. |
| The prune grace is at least one hour. | `validate_settings` in `src/config/validation.rs`, as `MIN_PRUNE_GRACE_SECONDS`. See *The floors that are not settings* in §1.3. |
| The log filter parses. | `validate_settings` in `src/config/validation.rs`. |
| The pagination default is positive and the maximum is at least the default. | `validate_settings` in `src/config/validation.rs`. |
| The rate limit is positive and its window is not zero. | `validate_settings` in `src/config/validation.rs`. |
| The CORS allowlist contains no wildcard, and every origin is a valid header value. | `validate_cors` in `src/config/validation.rs`. This used to be an assertion in the router. |
| The pool size is positive, and neither the acquire nor the connect timeout is zero. | `validate_settings` in `src/config/validation.rs`. |
| The access-token lifetime is positive and the refresh-token lifetime is strictly longer. | `validate_settings` in `src/config/validation.rs`. |
| The access-token lifetime is at most one day, and the refresh-token lifetime at most one year. | `validate_settings` in `src/config/validation.rs`, as `MAX_ACCESS_TOKEN_TTL_SECONDS` and `MAX_REFRESH_TOKEN_TTL_SECONDS`. See *The floors that are not settings* in §1.3. |
| The issuer and the audience are not empty. | `validate_settings` in `src/config/validation.rs`. |
| The Argon2 memory is at least 8 MiB, and the iteration count and the degree of parallelism are at least one. | `validate_settings` in `src/config/validation.rs`. |
| The password policy has a positive minimum and a maximum that is at least the minimum. | `validate_settings` in `src/config/validation.rs`. |
| The password minimum is at least twelve characters. | `validate_settings` in `src/config/validation.rs`, as `MIN_PASSWORD_LENGTH`. See *The floors that are not settings* in §1.3. |
| The clock-skew allowance is positive and at most five minutes. | `validate_settings` in `src/config/validation.rs`, as `MAX_CLOCK_SKEW_SECONDS`. See *The floors that are not settings* in §1.3. |
| `ml.dimension` is positive, `ml.timeout_seconds` is positive and not longer than the request timeout, and `ml.candidate_limit` is positive and at most `MAX_ML_CANDIDATE_LIMIT` (1,000). | `validate_ml` in `src/config/validation.rs`. The two settings bound different things: the timeout bounds one inference, and the re-rank performs none, so the candidate limit is the re-rank's only bound. A candidate set smaller than a page is a well-formed short page, not an error. |
| `ml.backend` is `flex` or `deterministic`. | The `MlBackend` type in `src/config/settings.rs`, for the file layer, and `MlBackend::parse` in `src/config/loader.rs`, for the environment layer. |
| The database can be reached, and the pending migrations can be applied. | `src/main.rs`, before the listener is bound. |

### 1.5 What the operator sees when the start-up fails

A configuration failure stops the process before the socket is bound, and the
message on stderr names the setting and what was expected. The operator sees
the **readable** form of the failure, not a debug rendering of an error
value:

```text
error: required environment variable DATABASE_URL is missing or empty
error: environment variable APP_MAX_BODY_BYTES has invalid value 0: expected a positive number of bytes
error: could not parse the configuration file app.toml: expected an equals, found an equals
```

The four shapes are the four variants of `ConfigError` in
[`../../src/config/validation.rs`](../../src/config/validation.rs): a missing
variable, an unacceptable value, an unreadable file and an unparsable file. A
rejected secret is reported by its **length**, never by its content:

```text
error: environment variable APP_JWT_SECRET has invalid value <16 bytes>: expected at least 32 bytes of entropy
```

The process exits with a non-zero status and never binds a port in a
partially configured state.

## Phase 2 settings — local machine learning

*The heading is unnumbered on purpose:* the section sits between §1.5 and §2
without renumbering either, because §1.3, §5.1 and §6.2 are cited by number from
other documents.

The seven `ml.*` settings are implemented and are listed in the table of §1.3:
`ml.enabled`, `ml.backend`, `ml.model_path`, `ml.tokenizer_path`,
`ml.dimension`, `ml.timeout_seconds` and `ml.candidate_limit`. They are the
whole configuration surface of the feature: Phase 2 adds **no new process, no
new service, no new secret and no credential for a third party**
([`../features/ml/architecture.md`](../features/ml/architecture.md),
[`../decisions/ADR-0008-local-pure-rust-ml.md`](../decisions/ADR-0008-local-pure-rust-ml.md)).

Four properties are worth stating here, because they are operational rather than
internal:

- **The weights are a local file.** A SafeTensors file is placed on the host by
  the operator. It is not in the repository, not baked into the image and not
  downloaded by the build or at run time; there is no outbound call to any
  hosted service, at build time or at run time.
- **A missing model file does not stop the process.** It selects the `disabled`
  embedder, and every semantic or hybrid request answers `200` with lexical
  results and names the mode that was applied — the deliberate difference from
  every rule in §1.4, where a bad setting is a start-up failure. A model is an
  optional artifact, so its absence is a supported state, not a misconfiguration
  (`ML-R02`, `ML-R06`).
- **The Burn encoder exists and is measured (ADR-0008 §12.1).** `ml.enabled = true`
  with `ml.backend = flex` selects the Burn encoder and the WordPiece tokenizer
  when both artifacts load; a missing or unreadable artifact selects `disabled`
  with `model_missing`, and start-up still succeeds — the rule above. What
  remains outstanding is the independent review of the encoder span and the
  re-gate, not the code path or a measurement:
  [`../features/ml/status.md`](../features/ml/status.md) §1.3 is the record, and
  the requirement states are not restated here.
- **Adding one of the seven means all seven places in one change**, as
  [`../../AGENTS.md`](../../AGENTS.md) §4.11 requires: the file in `config/`, the
  typed structure, the loader, the validation, `.env.example`, the reference
  tables (this document and [`../../config/README.md`](../../config/README.md))
  and the tests. `check_settings_reference` in
  [`../../src/bin/atom-docs.rs`](../../src/bin/atom-docs.rs) now recognises `ml.`
  rows and `config/ml.toml` is in `SETTINGS_FILES`, so a key that lands in only
  some of them is reported instead of being silently unchecked.

## 2. Secrets

| Rule | Where it is enforced |
| ---- | -------------------- |
| The signing secret comes from the environment and has **no file layer**. | `required("APP_JWT_SECRET")` in `src/config/loader.rs`. |
| The database URL comes from the environment and has **no file layer**. | `required("DATABASE_URL")` in `src/config/loader.rs`. |
| **No file in `config/` may contain a secret.** | The directory is committed and embedded in the binary. A setting that needs a credential is an environment variable; the two secrets above are the only ones. |
| The signing secret is at least 32 bytes. | `validate_secret` in `src/config/validation.rs`. |
| The documented placeholder is refused in production. | `validate_secret` in `src/config/validation.rs`. |
| The connection string never appears in a log line or a response. | The manual `Debug` implementation of `DatabaseSettings`. |
| The signing secret never appears in a log line or a response. | The manual `Debug` implementation of `AuthSettings`, and the `Debug` implementation of `TokenService`, which does not render the key at all. |
| `.env` is git-ignored; `.env.example` holds placeholders only. | `.gitignore` |
| No secret is ever returned to a client. | `AppError::body` in `src/error.rs` |

A rejected secret is reported by its **length**, never by its content.

In production the variables are supplied by the platform's secret manager or
by the process environment. The application has **no** secret-manager client,
**no** key rotation and **no** way to reload a secret without a restart.

## 3. Local development

### 3.1 Prerequisites

- Rust stable with `rustfmt` and `clippy`. The toolchain is pinned by
  `rust-toolchain.toml`:
  `channel = "stable"`, `components = ["rustfmt", "clippy"]`.
- PostgreSQL 14 or newer, per the contributor guide, or Docker.

### 3.2 The database

`docker-compose.yml` brings up a **local PostgreSQL only**. It does **not**
build an application image: the API runs from the source tree.

```bash
docker compose up -d postgres     # start PostgreSQL 18 on 127.0.0.1:5432
docker compose ps
docker compose down                # stop, keep the data
docker compose down -v             # stop and delete the data
```

The compose file creates the `atom` database from `POSTGRES_DB` and, through
`scripts/create-test-database.sh`, the development database `atom_dev` and the
disposable `atom_test` database that the integration suite needs. The
credentials match `.env.example` and are for local development only.

`atom_dev` is the **development** database — `DATABASE_URL` in `.env.example`
points at it, and it is the name the API and the seeder use — while `atom_test`
is **disposable** and used by the integration suite, whose harness refuses any
database name that does not contain `test`; the seeder refuses `atom`
deliberately, so the local setup provides a name the guard already accepts
instead of loosening the guard.

Without Docker:

```bash
createdb atom_dev
createdb atom_test
```

### 3.3 The configuration and the run

```bash
cp .env.example .env          # Linux/macOS
Copy-Item .env.example .env   # Windows PowerShell
```

Replace at least `APP_JWT_SECRET` with a real value of at least 32 bytes.
`openssl rand -base64 48` produces one.

```bash
cargo run
```

The API listens on `http://127.0.0.1:3000` by default and applies the pending
migrations at start-up. A background task then prunes the refresh sessions
that expired more than a day ago, every hour — both the interval and the
grace period come from the configuration.

```bash
curl http://127.0.0.1:3000/health/live
# {"status":"live"}

curl http://127.0.0.1:3000/health/ready
# {"status":"ready","checks":{"database":"ok"}}
```

### 3.4 The quality gates

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
TEST_DATABASE_URL=postgres://atom:atom@127.0.0.1:5432/atom_test cargo test --all-targets
```

`make check` runs all three. The integration harness refuses to start unless
the database name contains `test`, so it cannot damage a real database.

## 4. A note on the body limit

`APP_MAX_BODY_BYTES` is installed as an Axum `DefaultBodyLimit`, and the
project's own `api::extract::Json` reads that limit through
`req.into_limited_body()`. The configured value is therefore the ceiling the
router applies to every request.

There is one further, independent cap. The extractor passes the constant
`MAX_BODY_BYTES_BACKSTOP` — 4 MiB — to `axum::body::to_bytes`, so a body is
refused when it exceeds **either** limit. The backstop is a constant in
`src/config/validation.rs` and not a setting: it is a safety ceiling of the
parser. The configuration validation therefore **refuses** a
`max_body_bytes` above 4 MiB, so a deployment that sets
`APP_MAX_BODY_BYTES` above it does not start at all; the operator sees the
bound named in the start-up error. The operator-facing rule is therefore:
keep `app.limits.max_body_bytes` at or below 4 MiB.

## 5. The production checklist

The API does not enforce most of these itself. They are operator obligations.

| # | Obligation | Why |
| - | ---------- | --- |
| 1 | **Terminate TLS in front of the API.** | The process speaks plain HTTP/1.1 and does not verify TLS. |
| 2 | **Run behind a trusted reverse proxy or load balancer.** | The API does not declare which proxies it trusts and ignores `X-Forwarded-For` entirely. |
| 3 | **Put edge rate limiting in front of the API.** | The in-process limiter is per process, and behind a proxy it is keyed by the proxy's address. |
| 4 | **Supply the signing secret from a secret manager.** | At least 32 bytes, and never the documented placeholder. |
| 5 | **Use a distinct database role with the least privilege it needs.** | The role must be able to create and alter objects, because the binary applies the migrations at start-up. |
| 6 | **Ensure the migrations are applied before the new code serves traffic.** | The binary applies them itself; with more than one instance, the first one to start applies them and the others wait. Do not run a rolling deployment against a schema the old binary cannot read. |
| 7 | **Back up the database, and test the restore.** | The data is the product. |
| 8 | **Ship the logs somewhere durable.** | The JSON formatter is the production format. |
| 9 | **Set `APP_ENV=production`.** | It selects `config/environments/production.toml` and enables the placeholder-secret check. |
| 10 | **Set `APP_CORS_ALLOWED_ORIGINS` to the real origins, or leave it empty.** | The committed default in `config/api.toml` is an empty list, which disables CORS, and the template in [`.env.example`](../../.env.example) ships an empty value too. The environment layer wins over the file layer, so any non-empty value in a `.env` **enables** CORS for those origins. A `*` entry aborts the process. |
| 11 | **Keep `APP_LOG_FILTER` at `info` or higher in production.** | A `debug` filter logs every rejected request, including the reason. |
| 12 | **Raise the Argon2 cost for the deployment.** | The defaults are a floor, not a tuned value for the target hardware. |
| 13 | **Decide whether the API explorer is served.** | `config/environments/production.toml` sets `api.docs_enabled` to `false`. Set `APP_API_DOCS_ENABLED=true` only if the deployment wants the OpenAPI document and the explorer. **The build and the setting must agree:** a `--no-default-features` build cannot serve them at all, because the route and its assets are not compiled in. Against such a binary `APP_API_DOCS_ENABLED=true` logs one warning at start-up and `/api/v1/docs` returns `404`. |
| 14 | **Size the pool, the timeouts and the body limit for the deployment.** | The production overlay sets `database.max_connections` to 20 and the request timeout to 30 s. The base files hold the body limit (1 MiB) and the acquire and connect timeouts. Keep the body limit at or below 4 MiB: a larger setting is refused at start-up, because the extractor applies an independent 4 MiB backstop. See §4. |
| 15 | **Keep the session prune grace at its default, or raise it — never shorten it below the floor.** | `app.sessions.prune_grace_seconds` is the window in which the row of an expired session is kept, so that a token that expired recently is still recognised as *expired*. The floor is one hour. |
| 16 | **Set the clock-skew allowance to what the hosts actually need.** | `APP_ACCESS_TOKEN_CLOCK_SKEW_SECONDS` is 30 s by default. Hosts that drift by more than that see unexplained `401` responses. |
| 17 | **Know the database server's `work_mem` before quoting a search latency.** | The `semantic` sort spills above ~2 700 note vectors at 4 MB and the same statement then costs 5× (14–16 ms → 75–123 ms). It is a PostgreSQL setting, not an API setting, and the API cannot read or change it. See §7.1. |

`config/environments/production.toml` exists and is applied whenever
`APP_ENV=production`. It changes five settings: the log filter, the request
timeout, the session prune interval, `api.docs_enabled` and
`database.max_connections`. Every one of them is still overridable by an
environment variable, because the environment variables are the last layer.

### 5.1 The release process

**There is no automated release process yet.** The quality gates run in CI
(`.github/workflows/ci.yml`) on every push and pull request to `master` or
`develop`, but there is no versioning of the artifact and no deployment
manifest, so a rollout is still a manual step. The migration policy
in [`../database/migration-policy.md`](../database/migration-policy.md) §5
describes the intended rollback rule — forward-fix, never edit — and it is the
constraint a future release process must honour.

**What to build.** The deployable artifact is built **without** the
documentation feature:

```text
cargo build --release --no-default-features
```

The `docs` Cargo feature is on by default and carries the Swagger UI asset
bundle — 10.73 MiB embedded by `rust-embed`. Without it the binary is
**7.40 MiB instead of 19.43 MiB** of the same code, measured on this tree. A
deployment that wants the API explorer at `/api/v1/docs` builds with the default
features instead and accepts the larger binary.

### 5.1a The container image

`Dockerfile` at the repository root builds the deployable artifact. It is a
two-stage build: `rust:1.99.1-slim-bookworm` compiles the release binary from the
committed lockfile with `--locked`, and `debian:bookworm-slim` carries the binary
and a CA bundle and nothing else. Both base images are pinned by tag **and** by
digest, because a floating tag would make the image unreproducible even with a
committed `Cargo.lock`.

```text
docker build --tag atom-api:local .
docker run --rm -p 3000:3000 \
  -e DATABASE_URL='postgres://…' \
  -e APP_JWT_SECRET='…' \
  atom-api:local
```

Four properties the image has, each of which someone will be tempted to undo:

| Property | Why it is deliberate |
| -------- | -------------------- |
| The process runs as **UID 10001**, never root | A container escape from a root process is a host compromise; from an unprivileged one it is not. |
| `migrations/` is **not** copied in | The migrations are embedded by `sqlx::migrate!` and applied at start-up, before the socket binds. A second copy in the image would be a second source of truth, and an entrypoint that migrated would need a database client the image does not ship. |
| `config/` is **not** copied in | The configuration is embedded at compile time with `include_str!`. A deployment overrides a setting with an environment variable or `ATOM_CONFIG_DIR`, never with a file baked into the image. |
| **No secret is in the image** | `DATABASE_URL` and `APP_JWT_SECRET` are runtime environment variables. No `ENV` sets one, no build argument carries one, and `.dockerignore` keeps `.env*` out of the build context. A build argument would not help: it is visible in the image history. |

The image is built **without** the `docs` feature (see *What to build* above), so
the container is the 7.40 MiB artifact rather than the 19.43 MiB one.

`.github/workflows/container.yml` builds the image and then **verifies** it: the
declared user is not root, `id` reports `uid=10001`, `cargo` and `rustc` are
absent from the runtime stage, `ldd` reports no missing shared library, and the
binary exits non-zero when started without `DATABASE_URL` — which is the evidence
that no configuration or secret was compiled in. The workflow never logs in to a
registry and never pushes: publishing an image is a release decision.

### 5.2 The precedence an operator must remember

Three cases trip people up, because the committed file, the committed
template and the environment overlay do not always agree:

| Setting | The base file says | The template says | What the process does |
| ------- | ------------------ | ----------------- | --------------------- |
| `api.cors.allowed_origins` | `[]` — CORS off | empty — CORS off | The same. A `.env` that carries a non-empty value **enables** CORS for those origins, because the environment layer wins. |
| `api.docs_enabled` | `true` | `true` | The production overlay sets it to `false`, and the template overrides the overlay again, so a `.env` copied from the template **publishes** the OpenAPI document in production unless the value is cleared. |
| `app.log_filter` | `info` | `info` | The same, unless the overlay or the variable says otherwise. |

The rule is the same in all three cases: **the environment layer wins over the
file layer, and the file layer wins over the compiled default.** A `.env` is
not a neutral copy of the committed defaults — it is an override.

## 6. Observability

### 6.1 Structured logs

`src/telemetry.rs` installs the tracing subscriber at start-up:

| `APP_ENV` | Formatter |
| --------- | --------- |
| `production` | JSON, with a flattened event. |
| Anything else | Human-readable, with the target and the level. |

The formatter follows the environment, not the build profile: a release build
that runs a development deployment gets readable logs, and a debug build that
runs a production deployment gets JSON.

The filter comes from the configuration: `app.log_filter`, which the
environment variable `APP_LOG_FILTER` overrides and the environment overlay
can replace. An invalid filter is a start-up error, not a silent fallback to
`info`.

What the API logs:

| Event | Level | Content |
| ----- | ----- | ------- |
| Start-up | `info` | The bind address and the environment. |
| Migrations applied | `info` | A fixed message. |
| Listening | `info` | The bind address. |
| Shut-down | `info` | The signal that was received. |
| A request | `debug` | The `TraceLayer` span and event. At the default `info` filter the trace layer is silent. |
| A rejected request | `debug` | The `AppError` and the status code. |
| A server error | `error` | The `AppError`. A database failure is logged with its SQLSTATE and the error kind only, never with the driver message. |
| A panic | `error` | A fixed message. The payload is deliberately not logged. |
| Expired sessions pruned | `info` | The number of removed rows. |
| Pruning failed | `warn` | The SQLSTATE only. |

What the API never writes to a log: a password, a token, the signing secret
or the connection URL. `DatabaseSettings` and `AuthSettings` redact the two
values that a configuration dump would otherwise carry, the password is never
stored in the clear and never logged, and the panic handler deliberately
drops the panic payload.

### 6.2 The correlation id

Every request carries an `x-request-id` header, and every response echoes
it. A caller may supply its own value; the API keeps it and generates a
UUIDv4 only when the request has none. The full flow is in
[`request-lifecycle.md`](request-lifecycle.md) §8.

The limitation to know about: the id is carried by the header and the
tracing span. It is **not** currently written as an explicit field on every
log line, so correlating a log line with a request means relying on the
tracing span context.

### 6.3 The health endpoints

| Endpoint | Meaning | Failure |
| -------- | ------- | ------- |
| `GET /health/live` | The process can serve requests. | Never fails while the process answers. |
| `GET /health/ready` | The process can serve requests **and** the database answered a `SELECT 1`. | `503` with `{"status":"degraded","checks":{"database":"unavailable"}}`. |

Both are outside `/api/v1` and outside the authentication boundary, so an
orchestrator can probe them without credentials. The readiness probe is
bounded by `app.limits.readiness_probe_timeout_seconds` — one second by
default — so a stalled database cannot hang it. The liveness probe does not
touch the database, so a database outage does not cause an orchestrator to
restart the process.

## 7. Scaling characteristics

| Dimension | Behaviour today |
| --------- | --------------- |
| Horizontal | The API is stateless apart from the in-process rate limiter, so several instances can serve the same database. The rate limiter's budget is then multiplied by the number of instances. |
| Vertical | One Tokio task per connection. The pool size (`database.max_connections`, 10 by default and 20 in the production overlay) bounds the concurrency against the database. |
| The database | The pool is the bottleneck. `database.acquire_timeout_seconds` bounds how long a request waits for a connection; a request that cannot get one fails with `database_error`. |
| The database, semantic search | `semantic` mode adds one statement whose cost is linear in the **`embeddings` table**, not in the caller's result: 177–292 ms for 8 100 vectors at 20 000 rows ([`../database/performance.md`](../database/performance.md) §3.5). `hybrid` is the bounded alternative, capped by `ml.candidate_limit`; `semantic` is bounded by volume rather than by configuration, and [`ADR-0010`](../decisions/ADR-0010-semantic-search-scan-bound.md) accepts that to 100 000 rows. See §7.1 for the one server setting that changes it. |
| CPU | The dominant cost is Argon2id, which is deliberately expensive. It is bounded by the password length cap and by the rate limit. With `ml.enabled = true` one inference is 88 ms on the reference machine ([`../decisions/ADR-0008-local-pure-rust-ml.md`](../decisions/ADR-0008-local-pure-rust-ml.md) §12.1). |
| Memory | The connection pool, the in-flight bodies (capped at `APP_MAX_BODY_BYTES` each) and the rate limiter's map, which is swept of expired clients at most once per window and therefore holds only the clients of the last two windows. A `semantic` search additionally holds the caller's vectors in the request: 1 536 bytes each, so ~12 MB at 8 100 vectors, per concurrent request. |

**The rate limiter is the one component that does not scale horizontally.**
It is per process, in memory, and its key is the peer address of the
connection. Behind a proxy that does not terminate the connection, every
request carries the proxy's address and the budget is shared by every
caller. A multi-instance deployment therefore needs edge rate limiting. Both
are recorded in [`../engineering/known-issues.md`](../engineering/known-issues.md)
KI-001 and KI-003.

### 7.1 The database setting the API depends on: `work_mem`

One PostgreSQL setting changes this API's latency by a factor of five, and it is
**not** in `config/`. An operator who tunes the API and not the database will
spend a long time looking in the wrong place.

- **What it affects.** The `semantic` search's statement ends with
  `ORDER BY e.source_id` over the caller's note vectors, and each row is a
  384-element `REAL[]` of about 1.5 kB (`NOTE_VECTORS`,
  [`../../src/ml/repository.rs`](../../src/ml/repository.rs)). 8 100 vectors is
  about 12 MB of sort input.
- **The cliff.** With `work_mem = 4 MB` — the value the measurements used — the
  sort stops fitting at about **2 700 vectors per owner** and becomes an
  `external merge` with temporary files. Across that point the same statement
  goes from **14–16 ms to 75–123 ms**
  ([`../database/performance.md`](../database/performance.md) §4). The
  177–292 ms at 20 000 rows includes that spill; a deployment with a larger
  `work_mem` pays less of it.
- **How to raise it.** Scope it to the API's role or database, not to the
  server:

  ```sql
  ALTER ROLE atom SET work_mem = '16MB';
  ```

  16 MB holds roughly 10 000 vectors of 1.5 kB. `work_mem` is **per sort node
  per connection**, so a server-wide change multiplies memory for every
  connection of every application on that server, while the API's own pool is 10
  connections by default and 20 in the production overlay
  (`database.max_connections`).
- **Why there is no `database.work_mem` setting.** It is a property of the
  database and its host, not of the API's configuration. Issuing a
  per-transaction `SET work_mem` from the API would be a planner hint with a
  configuration key in front of it, and it would make latency depend on
  connection-pool bookkeeping rather than on the server. `AGENTS.md` §4.11's rule
  does not apply: it governs a value **the API** resolves, and this one is
  resolved by PostgreSQL.
- **Record it with any benchmark.** A semantic-search latency quoted without
  `work_mem` is not reproducible across the spill point.
  [`../database/performance.md`](../database/performance.md) §2.1 records the
  server settings for exactly this reason.
- **It is latency, not behaviour.** The same rows match and the same rows are
  returned at either setting, and ownership is unaffected. Nothing here is
  allowed to become a correctness claim.
- **The sort is removable, and that is a code change.** The statement orders by
  `source_id`, and the ranking in Rust re-sorts with a total order
  ([`../../src/ml/service.rs`](../../src/ml/service.rs)), so the `ORDER BY`
  decides nothing about the response. Removing it is named in
  [`ADR-0010`](../decisions/ADR-0010-semantic-search-scan-bound.md) and is not
  authorised by it. Until that change lands, `work_mem` is the knob.

The accepted cost of the semantic scan, and the volume at which this stops being
acceptable, are [`ADR-0010`](../decisions/ADR-0010-semantic-search-scan-bound.md).

## 8. What is **not** implemented yet

| Thing | State | Consequence |
| ----- | ----- | ----------- |
| A container image for the API | Not implemented | The service is built and run from the source tree. `docker-compose.yml` is for the database only. |
| A release process, an artifact version and a deployment manifest | Not implemented | There is no supported way to roll the service out. The quality gates run in CI; the rollout is manual. |
| A runtime configuration reload | Not implemented | The configuration is resolved once, at start-up. A changed file or variable needs a restart. |
| A secret-manager client and a key-rotation procedure | Not implemented | The signing secret is fixed for the lifetime of the process. |
| An edge rate limiter or a shared limiter | Not implemented | The in-process limiter is the only control, and it is per process. |
| TLS termination in the process | Not implemented | A proxy is required. |
| Metrics, tracing spans exported to a collector | Not implemented | Only logs are emitted. |
| A database backup and restore procedure | Not implemented | The checklist in §5 requires one. |

## 9. Related documents

- [`../../config/README.md`](../../config/README.md) — the configuration
  reference: the layers, the files, the catalogue and the validation rules.
- [`system-overview.md`](system-overview.md) — the runtime shape.
- [`security-architecture.md`](security-architecture.md) — the operator's
  security obligations.
- [`request-lifecycle.md`](request-lifecycle.md) — what happens to a request.
- [`../database/migration-policy.md`](../database/migration-policy.md) — how
  the schema is changed and released.
- [`../engineering/known-issues.md`](../engineering/known-issues.md) — the
  confirmed limitations.
- [`../decisions/ADR-0006-configuration-source-of-truth.md`](../decisions/ADR-0006-configuration-source-of-truth.md)
  — the decision behind the three layers.
- [`../../README.md`](../../README.md) — the quick start.
- [`../../CONTRIBUTING.md`](../../CONTRIBUTING.md) — the contributor guide.
