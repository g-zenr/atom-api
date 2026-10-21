#!/usr/bin/env bash
#
# Checks the ATOM documentation tree for consistency.
#
# Runs the `atom-docs` binary, which is built from the repository's own Rust
# toolchain. It checks that:
#   * every required document exists
#   * every relative Markdown link resolves
#   * every feature status uses one of the seven defined status words
#   * every ADR has a status and a unique identifier
#   * every change-log identifier is unique
#   * every feature directory has its five required documents
#
# Usage: ./scripts/check-docs.sh [--strict]

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1

ARGS=(run --quiet --bin atom-docs)
if [ "${1:-}" = "--strict" ]; then
  ARGS+=(--strict)
fi

cargo "${ARGS[@]}"
exit $?
