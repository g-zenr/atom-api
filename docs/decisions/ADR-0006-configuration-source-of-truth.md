# ADR-0006 — The configuration directory is the source of truth

> **Status:** Accepted
> **Date:** 2026-10-22
> **Owner:** architect

## Context

Until this decision, every setting of the ATOM API was an environment
variable, read exactly once at start-up by `Config::from_env` in
`src/config.rs`. The typed `Config` was then handed to the rest of the
application. The model was simple and it worked, but it had three problems
that the growth of the API made visible.

**The set of settings was no longer in one place.** The reference table in
[`../architecture/deployment.md`](../architecture/deployment.md) §1 listed
twenty-one variables, and the values themselves lived in the environment of
whatever machine the process happened to run on. A reviewer could not answer
"what does this service do with a request body of 8 MiB?" by reading the
repository; the answer was in a shell profile somewhere else. Every new
setting added a row to a table, an entry to `.env.example` and a branch in
`Config::from_env`, and the three had to be kept in step by hand.

**The client-facing text was scattered through the code.** Every error
message that reached a caller was a string literal in a handler, a service or
an extractor. The wording was therefore not reviewable in one place, and it
could not be changed without touching Rust code and rebuilding the binary.

**The validation rules were spread over two layers.** Some were checked in
`src/config.rs` and some were checked where they were used — the CORS
allowlist, for example, was parsed but never validated against a real origin,
and an invalid entry tripped an `assert!` in the router. A rule that lives
next to the code that uses it is a rule that is checked late, or not at all.

The constraints that shape the answer are these:

- the project must stay usable from a clone
  ([`../project-truth.md`](../project-truth.md) §3), so the configuration
  cannot live in a secret manager or in a service;
- the process must never serve traffic in a partially configured state
  (`src/main.rs`), so the configuration must be resolved and validated before
  the socket is bound;
- a secret must never enter the repository
  ([`../../AGENTS.md`](../../AGENTS.md) §8), so the configuration directory
  must be safe to commit — which means it may not hold a credential;
- the process is stateless and is deployed without a container image or a
  deployment manifest, so the configuration must not require files that ship
  next to the binary.

The change also touched an accepted decision. ADR-0004
([`ADR-0004-documentation-as-project-truth.md`](ADR-0004-documentation-as-project-truth.md))
states, in its *Security and operational implications*:

> "**The published configuration reference is the operator's contract.**
> [`../architecture/deployment.md`](../architecture/deployment.md) §1 is the
> single list of the environment variables, so a secret that is not in that
> table does not exist."

That claim was true when every setting was an environment variable. It is no
longer true: the configuration now has a file layer that sits **below** the
environment variables, and the operator's contract is a table of *keys*
rather than a list of variables. Under the rule in
[`README.md`](README.md) §1.1 — an ADR is never rewritten, a decision that
narrows an earlier one is superseded — this ADR replaces that claim.

## Decision

**The root `config/` directory is the single source of truth for the
application's non-secret settings. The configuration is resolved once, at
start-up, from three documented layers, and the two secrets are held only in
the environment.**

### 1. The layers

```text
1. the base files in config/          app, api, auth, database, messages
2. the environment overlay            config/environments/<APP_ENV>.toml
3. the environment variables          the names in .env.example
```

A later layer wins over an earlier one, **key by key**. A value that a layer
does not set keeps the value from the layer below it. The environment overlay
is selected by `APP_ENV`, which accepts `development` (`dev`, `local`) or
`production` (`prod`) and defaults to `development`.

### 2. Where the files come from

The files are **embedded in the binary**, so a deployment needs no
configuration directory. The loader looks for a directory in this order: the
path in `ATOM_CONFIG_DIR`, a `config` directory next to the executable, then
a `config` directory in the working directory. A file found on disk is merged
**on top of** the embedded copy, key by key; a file that is absent leaves the
embedded copy in place. `ATOM_CONFIG_DIR` that points at a directory which
does not exist is an error.

### 3. The secrets have no file layer

`DATABASE_URL` and `APP_JWT_SECRET` are required environment variables. They
are in no file, and **no file in `config/` may contain a secret**. The
directory is committed and embedded in the binary, so anything in it is
public. A setting that needs a credential is an environment variable.

### 4. One resolution, one validation

The configuration is resolved once, in `src/config/loader.rs`, into a typed
`Config`, and validated once, in `src/config/validation.rs`, **before the
socket is bound**. A rule that used to be enforced where the value was used
— the CORS allowlist — is enforced there now, as a `ConfigError` rather than
an assertion in the router.

### 5. The client-facing text is a file too

Every string that reaches a client through the error envelope comes from
`config/messages.toml`, resolved at the call site. The **stable error code**
is not in that file: the code is part of the API contract and lives in the
`code` module in `src/error.rs`, with exactly one definition per code. The
`Display` text on `AppError` and the `tracing` log messages stay in the
code, because they never reach a client.

### 6. The rules for a new setting

A setting that a deployment may need to change at deploy time also gets an
environment variable. A value the database enforces stays a constant in the
code. A value that is part of the published client contract — the pagination
limits — stays in a file, and changing it is a breaking change. The rules
are in [`../../AGENTS.md`](../../AGENTS.md) §4.11 and in
[`../../config/README.md`](../../config/README.md) §7.

## Alternatives considered

| Alternative | Why it was not chosen |
| ----------- | --------------------- |
| **Environment variables only** — the status quo: `Config::from_env` reads every setting from the process environment. | It was simple, and it was the reason the decision was taken. It puts the whole configuration outside the repository, so a reviewer cannot see what the service does with a given value, and it makes the configuration a per-machine affair that is never reviewed. It also has no natural place for a value that only varies by environment: the differences between development and production had to be expressed as separate environment variables on separate machines, which is exactly the difference the overlay file now expresses. |
| **A single TOML file** — one `config.toml` at the root of the repository. | One file makes the differences between environments impossible to express without editing the committed file, which is the first thing an operator does and the last thing that should be committed. It also puts every area's settings in one namespace, so the file grows into a single long document that nobody owns. The five base files and the two overlays keep the settings grouped by the area that owns them, and they let an operator override one file without touching the others. |
| **A configuration framework** — `config`, `figment` or `rust-confight`, with the layering, the formats and the environment overrides provided by the crate. | A framework would have removed the merging, the discovery and the override code, which is real value. It was not chosen because it would also have defined the *shape* of the configuration — its own key syntax, its own environment-variable conventions and its own error type — and those conventions would then be the project's, whether or not they fitted. The rule in [`../architecture/README.md`](../architecture/README.md) §5 is that every name in the documentation must exist in the repository; a framework's conventions live in the framework's documentation. The loader is a few hundred lines and it is testable against the real files. |
| **YAML** — the configuration in YAML instead of TOML. | YAML needs a second parser for no gain: the configuration is a flat, two-level structure of scalars and small tables, which TOML expresses directly. TOML also has the advantage of being a subset of a syntax that is already in the toolchain, and of reading the same in the Rust source (as a string literal) as in the file. The cost of the decision is a dependency on `toml`, which is recorded in the change log. |
| **Reading the configuration per request** — the files and the environment are read again for every request, so that a change takes effect without a restart. | It would move the failure modes of configuration into the request path, where they become per-request errors instead of a start-up refusal, and it would make the request path depend on the file system and on the process environment. It would also make the configuration mutable, which the current design deliberately forbids: the state is built once and the settings cannot change while the process runs. The cost is paid only by an operator who wants a change to take effect without a restart, and a restart is a normal deployment event. |

## Consequences and trade-offs

### Positive

- **The configuration is in the repository, and it is reviewable.** A reader
  sees the settings, their defaults and their constraints in one place, and
  a change to them is a pull request.
- **The environment differences are explicit.** The development and
  production overlays are two small files whose contents are the whole
  difference between the two environments.
- **A deployment needs nothing next to the binary.** The files are embedded,
  so there is no configuration directory to ship, and a directory that *is*
  shipped is merged on top of the embedded copy rather than replacing it.
- **The validation is in one place, before the socket is bound.** Every rule
  that used to be enforced where the value was used is enforced once, at
  start-up, and a violation is a `ConfigError` with the offending key.
- **The client-facing text is reviewable without touching Rust.** The wording
  a caller sees is a file, and the stable codes stay in the code, where a
  client can rely on them.
- **The secrets are separated by construction.** The two credentials have no
  file layer at all, so a secret cannot be committed by accident: the
  directory holds no secret to commit.

### Negative

- **There are three places to look.** A reader who wants the effective value
  of a setting has to know the precedence model, not just the file. The
  precedence is stated once, in this ADR, in
  [`../../config/README.md`](../../config/README.md) §2 and in
  [`../architecture/deployment.md`](../architecture/deployment.md) §1.
- **A setting is now added in six places.** The file, the typed structure,
  the loader, the validation, `.env.example` and the reference table. A
  contributor who adds a setting and forgets one of them gets a silent
  behaviour difference rather than a compile error. The rules in
  [`../../AGENTS.md`](../../AGENTS.md) §4.11 exist because of this.
- **A typo in a file is a start-up error.** The settings structures use
  `deny_unknown_fields`, so an unknown key stops the process instead of
  being ignored. That is deliberate, and it means a hand-edited file must be
  correct before the service starts.
- **The deployment contract is a table of keys, not a list of variables.** An
  operator who scripted a deployment against the old variable list has to
  check the mapping. The table in
  [`../architecture/deployment.md`](../architecture/deployment.md) §1.3 gives
  the variable for every key, so the mapping is one column.
- **The framework was not used, so the merging code is ours.** The
  discovery, the merge, the override and the error type are all in
  `src/config/loader.rs`, and they are ours to maintain.
- **The configuration still cannot change while the process runs.** The
  decision does not add a reload; it changes where the values come from.

## Security and operational implications

- **The configuration directory is a public artefact.** It is committed and
  embedded in the binary, so no file in it may contain a secret. The two
  credentials — the signing secret and the database URL — have no file layer
  and are environment variables only. A rejected secret is reported by its
  length, never by its content, and the two settings types that carry one
  print `[REDACTED]` in their `Debug` output.
- **The operator's contract is a key reference.** The table in
  [`../architecture/deployment.md`](../architecture/deployment.md) §1.3 is the
  complete list of the keys, their defaults, the environment variable that
  overrides each of them and the constraint each of them obeys. A setting
  that is not in that table does not exist. This is the same rule ADR-0004
  set, applied to keys instead of variables.
- **A misconfiguration is a start-up refusal, not a runtime surprise.** The
  validation covers the durations, the limits, the token lifetimes, the
  Argon2 cost, the password policy, the CORS allowlist, the pool and the
  timeouts. A wildcard origin is refused as a `ConfigError`, before the
  socket is bound, instead of by an assertion in the router.
- **The environment variables remain an override, not a second source of
  truth.** They are the last layer, so an operator can change any single
  setting at deploy time without editing a file, and the committed value is
  still the documented default.
- **What the operator does differently.** The secrets are supplied the same
  way as before — from the platform's secret manager or the process
  environment. What is new is that a deployment may also ship a `config/`
  directory, and that `APP_ENV=production` selects
  `config/environments/production.toml`, which turns the API explorer off,
  raises the pool size to 20, prunes the sessions every fifteen minutes and
  pins the log filter to `info`.
- **The performance effect is negligible.** The configuration is read once,
  at start-up, and the message catalogue is behind an `Arc` that the services
  share. The catalogue is read on every error path, not on every request.
- **The failure modes are the same as before, and fewer.** A missing
  variable, an unreadable file, an unparsable file, an unknown key and a
  rule violation all stop the process before it binds a port.

## Related requirements and changes

| Reference | Relationship |
| --------- | ------------ |
| [`../../AGENTS.md`](../../AGENTS.md) §4.11 | The rule that a setting is inspected before it is introduced, and the six places it has to be added. |
| [`../../AGENTS.md`](../../AGENTS.md) §8 | The rule that no file in `config/` may contain a secret. |
| [`../project-truth.md`](../project-truth.md) §3 | The requirement that the project is usable from a clone. |
| [`../architecture/deployment.md`](../architecture/deployment.md) §1 | The operator's contract: the layers, the key reference, the start-up contract and the secrets. |
| [`../../config/README.md`](../../config/README.md) | The configuration reference: the layers, the precedence, every setting, the secrets, the catalogue and the validation rules. |
| [`../architecture/module-boundaries.md`](../architecture/module-boundaries.md) §7.1 | The `config` module: its interface, its invariants and its error variants. |
| [`../architecture/request-lifecycle.md`](../architecture/request-lifecycle.md) §2 | The `RequestContext` extension that carries the catalogue and the request limits. |
| [`../decisions/ADR-0004-documentation-as-project-truth.md`](ADR-0004-documentation-as-project-truth.md) | The decision this one narrows. |
| [`../engineering/change-log.md`](../engineering/change-log.md) | The change that implements this decision. |

## Supersedes / Superseded by

Supersedes: ADR-0004, in the narrow sense that its claim — "the published
configuration reference is the operator's contract, and it is the single list
of the environment variables" — no longer describes the system. The rest of
ADR-0004 stands: the documentation tree is still part of the product, the
authority order is unchanged, and the operator's contract is still
published in [`../architecture/deployment.md`](../architecture/deployment.md)
§1. It is now a list of **keys** and their environment-variable overrides
rather than a list of variables.

Superseded by: none. A review of this decision is due when the project needs
a configuration source that is neither a file nor the environment — a remote
or managed configuration service — or when the secrets need a source other
than the process environment.
