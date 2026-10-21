---
description: Run the full quality gate and report the real result.
agent: quality-gate
---

Run the verification suite for the current repository state and judge the
evidence.

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
TEST_DATABASE_URL=postgres://atom:atom@127.0.0.1:5432/atom_test cargo test --all-targets
```

Then check the documentation against the implementation:

- `docs/project-truth.md` still describes the accepted scope
- every affected feature `status.md` reflects reality
- `docs/engineering/implementation-status.md` agrees with the feature documents
- the OpenAPI annotations describe the routes that exist
- the change log records this change with the checks that were actually run
- review findings are recorded and resolved

Return a verdict of exactly `PASS` or `FAIL`, with the commands you ran, their
real output, the documentation findings, and the limitations of what could not
be verified.

Do not modify any file.
