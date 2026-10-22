#!/usr/bin/env bash
#
# Validates the OpenCode configuration of this repository.
#
# Runs the real OpenCode CLI checks:
#   1. opencode --version        the installed version
#   2. opencode debug config     every configuration source, in precedence order
#   3. opencode debug agents     every discovered agent
#   4. model availability        is the configured default model offered?
#   5. JSON/JSONC well-formedness
#   6. secret scan
#
# Exit code 0 means every check passed.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1

SKIP_MODEL_CHECK="${1:-}"

failures=()
warnings=()

step() { printf '\n==> %s\n' "$1"; }
ok()   { printf '    OK   %s\n' "$1"; }
warn() { printf '    WARN %s\n' "$1"; warnings+=("$1"); }
fail() { printf '    FAIL %s\n' "$1"; failures+=("$1"); }

if ! command -v opencode >/dev/null 2>&1; then
  step "OpenCode version"
  fail "the 'opencode' CLI is not on PATH"
else
  step "OpenCode version"
  version="$(opencode --version 2>&1 | head -n 1)"
  ok "installed: $version"
  case "$version" in
    *"v2."*) : ;;
    *) warn "this configuration targets OpenCode v2 (permissions/agents/policies). Detected: $version" ;;
  esac
fi

step "Configuration sources"
if command -v opencode >/dev/null 2>&1; then
  if config_raw="$(opencode debug config 2>&1)"; then
    ok "opencode debug config succeeded"
    if printf '%s' "$config_raw" | grep -q '"type": *"document"'; then
      printf '%s' "$config_raw" \
        | tr ',' '\n' \
        | grep -o '"path": *"[^"]*"' \
        | sed 's/"path": *"/    loaded: /; s/"$//' || true
    fi
  else
    fail "opencode debug config failed"
    printf '%s\n' "$config_raw"
  fi
fi

step "Agent discovery"
if command -v opencode >/dev/null 2>&1; then
  if agents_raw="$(opencode debug agents 2>&1)"; then
    expected="orchestrator architect rust-backend auth-security database test-engineer code-reviewer quality-gate docs devops"
    for name in $expected; do
      # A herestring is used instead of a pipe: `grep -q` exits as soon as it
      # matches, which would make the pipeline fail under `set -o pipefail`.
      if grep -q "\"id\": *\"$name\"" <<<"$agents_raw"; then
        ok "agent: $name"
      else
        fail "agent not discovered: $name"
      fi
    done
    for name in code-reviewer quality-gate; do
      if printf '%s' "$agents_raw" | awk -v agent="\"id\": \"$name\"" '
            $0 ~ agent { found = 1 }
            found && /"action": *"edit"/ { found2 = 1 }
            found && /"effect": *"deny"/ && found2 { deny = 1 }
            END { exit deny ? 0 : 1 }'; then
        ok "read-only enforced: $name"
      else
        fail "$name may be able to edit files; a reviewer must be read-only"
      fi
    done
  else
    fail "opencode debug agents failed"
  fi
fi

step "Model availability"
if command -v opencode >/dev/null 2>&1 && [ "$SKIP_MODEL_CHECK" != "--skip-model-check" ]; then
  model=""
  if [ -f opencode.jsonc ]; then
    model="$(grep -o '"model"[[:space:]]*:[[:space:]]*"[^"]*"' opencode.jsonc | head -n 1 | sed 's/.*"\([^"]*\)"$/\1/')"
  fi
  if [ -z "$model" ]; then
    warn "no default model found in opencode.jsonc"
  else
    models_raw="$(opencode models 2>&1)"
    if grep -qx "$model" <<<"$models_raw"; then
      ok "model offered by the catalog: $model"
    else
      fail "the configured model is not in the catalog: $model"
      echo "    Run 'opencode models' to see what your account can use."
      echo "    Override the model in .opencode/opencode.jsonc - see docs/contributing-opencode.md."
    fi
  fi
fi

step "Configuration file well-formedness"
for file in opencode.jsonc .opencode/opencode.jsonc; do
  [ -f "$file" ] || continue
  if command -v node >/dev/null 2>&1; then
    # Strip JSONC comments without touching a "//" inside a string literal,
    # then remove the trailing commas JSONC allows.
    if node -e '
      const fs = require("fs");
      const src = fs.readFileSync(process.argv[1], "utf8");
      let out = "";
      let i = 0;
      let inString = false;
      let escaped = false;
      while (i < src.length) {
        const c = src[i];
        if (inString) {
          out += c;
          if (escaped) escaped = false;
          else if (c === String.fromCharCode(92)) escaped = true;
          else if (c === String.fromCharCode(34)) inString = false;
          i += 1;
          continue;
        }
        if (c === String.fromCharCode(34)) {
          inString = true;
          out += c;
          i += 1;
          continue;
        }
        if (c === "/" && src[i + 1] === "/") {
          while (i < src.length && src[i] !== String.fromCharCode(10)) i += 1;
          continue;
        }
        if (c === "/" && src[i + 1] === "*") {
          i += 2;
          while (i < src.length && !(src[i] === "*" && src[i + 1] === "/")) i += 1;
          i += 2;
          continue;
        }
        out += c;
        i += 1;
      }
      JSON.parse(out.replace(/,(\s*[}\]])/g, "$1"));
    ' "$file" 2>/dev/null; then
      ok "parses: $file"
    else
      fail "does not parse: $file"
    fi
  else
    warn "node is not available; skipped the parse check for $file"
  fi
done

step "Secret scan"
secret_pattern='(api[_-]?key|secret|password|token)[[:space:]]*[:=][[:space:]]*["'"'"'][A-Za-z0-9_-]{16,}'
found_secret=0
for file in opencode.jsonc AGENTS.md CONTRIBUTING.md README.md .env.example .gitignore Cargo.toml; do
  [ -f "$file" ] || continue
  while IFS= read -r line; do
    if printf '%s' "$line" | grep -Eqi "$secret_pattern" \
       && ! printf '%s' "$line" | grep -Eqi 'example|placeholder|REDACTED|replace-me|change-me|your-|<'; then
      fail "possible secret in $file: $line"
      found_secret=1
    fi
  done < "$file"
done
[ "$found_secret" -eq 0 ] && ok "no credential-shaped values in the committed configuration"

printf '\n'
if [ "${#failures[@]}" -gt 0 ]; then
  printf 'FAILED: %d check(s) failed.\n' "${#failures[@]}"
  for f in "${failures[@]}"; do printf '  - %s\n' "$f"; done
  exit 1
fi

printf 'PASSED: all checks completed (%d warning(s)).\n' "${#warnings[@]}"
for w in "${warnings[@]:-}"; do [ -n "$w" ] && printf '  - %s\n' "$w"; done
exit 0
