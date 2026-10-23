## Summary

- What changed?
- Why is it needed?

## Scope

- Included:
- Explicitly excluded:

## Related issue

- Closes #<number> / Refs #<number> / N/A
- Audit finding (if any): `F-xx` in [`docs/audit/audit-report.md`](../docs/audit/audit-report.md)

## Implementation notes

- Important design choices and trade-offs.
- If a rule in [`AGENTS.md`](../AGENTS.md) was intentionally departed from, say
  which and why.

## Verification

Record the **actual** result of each command, not an expectation. Mark a check
`N/A` only with a reason.

- [ ] `cargo fmt --all -- --check` — PASS / FAIL / N/A
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` — PASS / FAIL / N/A
- [ ] `cargo test --all-targets` — PASS / FAIL / N/A — record the counts
- [ ] `cargo run --bin atom-docs` — PASS / FAIL / N/A
- [ ] `cargo run --bin atom-docs -- --strict` — PASS / FAIL / N/A
- [ ] `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-opencode.ps1` — PASS / FAIL / N/A
- [ ] Focused tests, integration tests or migration checks required by this change

Integration tests need a **disposable** database:

```text
TEST_DATABASE_URL=postgres://atom:atom@127.0.0.1:5432/atom_test
```

The harness refuses a database whose name does not contain `test`. Never point
it at development, UAT or production data.

**Not verified:** list anything you could not run, and why. A truthful "not
verified" is required; a false "passing" is not acceptable.

## Risk and rollback

- Risks:
- Rollback or mitigation:

## Data / migration impact

- [ ] No database migration or data change
- [ ] Migration included and reviewed — describe the forward-only rollout and
      the compatibility window. Migrations are append-only; an applied
      migration is never edited.

## Deployment notes

- DEV:
- UAT acceptance:
- PROD release/rollback notes:

## Reviewer checklist

- [ ] Diff reviewed; no unrelated changes
- [ ] Tests prove the intended behaviour, including the cross-account case
- [ ] Security and ownership boundaries preserved — `WHERE user_id = …` still
      literal in SQL, unowned resources still `404`
- [ ] Documentation, change log and review log updated where required
- [ ] No secrets, generated artifacts or build output committed
