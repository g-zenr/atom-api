# ATOM API — container image.
#
# Two stages:
#
#   builder   rust:1.99.1-slim-bookworm   compiles the release binary from the
#                                         committed lockfile (`--locked`).
#   runtime   debian:bookworm-slim        carries the binary, the CA bundle and
#                                         nothing else. No toolchain, no cargo,
#                                         no source tree, no build tooling.
#
# Both base images are pinned by tag **and** by digest. A floating tag would
# make the image unreproducible even with a committed `Cargo.lock`, because the
# compiler and the C library would still be whatever the registry served that
# day.
#
# ---------------------------------------------------------------------------
# What is deliberately absent, and why nobody should "fix" it
# ---------------------------------------------------------------------------
#
# * `migrations/` is NOT copied into the runtime image. The migrations are
#   embedded in the binary by `sqlx::migrate!("./migrations")` in
#   `src/main.rs`, which applies them at start-up, before the socket is bound.
#   Do not copy the directory into the runtime image and do not add a migration
#   step to the entrypoint: a second copy of the schema would be a second
#   source of truth, and an entrypoint that migrated first would need a
#   database client the image does not ship. The binary is the migration
#   runner, and the deployment checklist in `docs/architecture/deployment.md`
#   §5 item 6 is written against that fact.
#
# * `config/` is NOT copied into the runtime image. The configuration files are
#   embedded at compile time with `include_str!` (see `src/config/loader.rs`).
#   A deployment overrides a setting with `ATOM_CONFIG_DIR` or an environment
#   variable, never with a file baked into the image.
#
# * No secret is in the image. `DATABASE_URL` and `APP_JWT_SECRET` are runtime
#   environment variables and nothing else: no ENV directive below sets one, no
#   build argument carries one, and `.dockerignore` keeps `.env*` out of the
#   build context. An `ARG` would not help either — a build argument is visible
#   in the image history, so a secret passed that way is published with the
#   image. Supply both at `docker run` / deployment time.

# ---------------------------------------------------------------------------
# Stage 1 — build
# ---------------------------------------------------------------------------
FROM rust:1.99.1-slim-bookworm@sha256:86234e857a413004348b471d11a4ebe1aa92d2eae2b6b143cac68fd1ac3c48be AS builder

# `rust-toolchain.toml` asks for the `stable` channel, which is a moving
# target: left alone, rustup inside the image would download whatever stable
# happens to be current, and the build would stop being reproducible. This
# variable makes rustup use the toolchain the image tag pins — the exact
# version, installed in the image, nothing downloaded. Bumping the image tag
# means bumping this value in the same change.
ENV RUSTUP_TOOLCHAIN=1.99.1
ENV CARGO_TERM_COLOR=never

WORKDIR /build

# Only what the compiler reads: the manifests and the lockfile, the sources,
# and the two directories that are embedded in the binary at compile time
# (`config/` through `include_str!`, `migrations/` through `sqlx::migrate!`).
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
COPY config ./config
COPY migrations ./migrations

# `--locked` twice over: the lockfile is authoritative for resolution, and a
# stale `Cargo.lock` fails the build instead of being silently rewritten.
#
# `--no-default-features` leaves out the `docs` feature: the Swagger UI bundle
# is 10.73 MiB of static assets and `config/environments/production.toml` sets
# `api.docs_enabled = false`, so a deployment never serves a byte of it. This
# matches the release build in `.github/workflows/ci.yml`, which is the
# artifact the quality gates exercise. A deployment that wants the explorer
# must change this line deliberately, not by accident.
#
# `--bin atom-api` keeps the development tool `atom-docs` out of the build.
RUN cargo build --release --locked --no-default-features --bin atom-api

# 0555 is preserved by the COPY in the runtime stage: the file is readable and
# executable by everyone and writable by nobody, which is all it needs to be.
RUN install -D -m 0555 target/release/atom-api /opt/atom/bin/atom-api

# ---------------------------------------------------------------------------
# Stage 2 — runtime
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8 AS runtime

# The CA bundle comes from the builder stage rather than from `apt-get`, so the
# runtime stage needs no package manager and installs nothing at build time:
# the same pinned digest always produces the same bytes.
#
# The PostgreSQL connection uses `sqlx` with the `tls-rustls` feature, which
# trusts the Mozilla roots compiled into the binary (the `webpki-roots` crate),
# so database TLS does not read this file. It is here because it is the trust
# store anything else in the process would resolve — and because an operator
# debugging a certificate problem expects `/etc/ssl/certs` to exist.
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

# An unprivileged, static identity. The UID and GID are fixed numbers so that a
# volume or a security policy can name them without depending on the image.
RUN groupadd --gid 10001 atom \
    && useradd --uid 10001 --gid atom --no-create-home --home-dir /nonexistent \
        --shell /usr/sbin/nologin atom

COPY --from=builder /opt/atom/bin/atom-api /usr/local/bin/atom-api

# `config/app.toml` binds `127.0.0.1:3000`, which inside a container is the
# container's own loopback: reachable from the process, from nowhere else. This
# is the one non-secret value the image must set for the container to be
# usable; it stays overridable at run time like every other setting.
ENV APP_BIND_ADDR=0.0.0.0:3000

# The port the binary binds by default (`app.bind_addr` in `config/app.toml`,
# overridden above to 0.0.0.0). Documentation only — the socket is what matters.
EXPOSE 3000

# `src/main.rs` installs a SIGTERM handler and drains for
# `app.limits.shutdown_grace_seconds`. SIGTERM is the default stop signal, and
# stating it keeps that contract visible where the process is defined.
STOPSIGNAL SIGTERM

# The whole point of the runtime stage: nothing runs as root.
USER 10001:10001

ENTRYPOINT ["/usr/local/bin/atom-api"]
