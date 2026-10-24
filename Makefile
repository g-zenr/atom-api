# ATOM developer tasks.
#
# `make check` is what CI runs and what a contributor should run before
# opening a pull request.
#
# On Windows, either use `make` from a POSIX shell (Git Bash, WSL) or run the
# commands directly — see CONTRIBUTING.md.

DATABASE_URL ?= postgres://atom:atom@127.0.0.1:5432/atom_dev
TEST_DATABASE_URL ?= postgres://atom:atom@127.0.0.1:5432/atom_test

export DATABASE_URL
export TEST_DATABASE_URL

.PHONY: help
help:
	@echo "ATOM developer tasks:"
	@echo "  make setup        install the toolchain components and fetch dependencies"
	@echo "  make fmt          format the source tree"
	@echo "  make lint         run clippy with warnings denied"
	@echo "  make test         run the unit and integration tests"
	@echo "  make check        fmt --check + lint + test (the full quality gate)"
	@echo "  make run          start the API"
	@echo "  make docs-check   validate the documentation tree"
	@echo "  make opencode     validate the OpenCode configuration"
	@echo "  make migrate      apply pending migrations"
	@echo "  make db-up        start PostgreSQL with docker compose"
	@echo "  make db-down      stop PostgreSQL"

.PHONY: setup
setup:
	rustup component add rustfmt clippy
	cargo fetch

.PHONY: fmt
fmt:
	cargo fmt --all

.PHONY: fmt-check
fmt-check:
	cargo fmt --all -- --check

.PHONY: lint
lint:
	cargo clippy --all-targets --all-features -- -D warnings

.PHONY: test
test:
	cargo test --all-targets

.PHONY: check
check: fmt-check lint test

.PHONY: build
build:
	cargo build --release

.PHONY: run
run:
	cargo run

.PHONY: migrate
migrate:
	cargo sqlx migrate run

.PHONY: docs-check
docs-check:
	cargo run --quiet --bin atom-docs

.PHONY: opencode
opencode:
	./scripts/validate-opencode.sh

.PHONY: db-up
db-up:
	docker compose up -d postgres

.PHONY: db-down
db-down:
	docker compose down
