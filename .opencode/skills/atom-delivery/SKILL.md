---
name: Atom Delivery
description: Use when implementing any substantial change to the ATOM backend. Defines the end-to-end delivery workflow from intake to verified, documented, reviewed result.
---

# ATOM delivery workflow

Follow this workflow for every substantial coding task. Small changes (a
one-line fix, a typo, a comment) may use a reduced form: implement, run the
targeted test, update the changelog.

## 1. Intake

1. Read `AGENTS.md`.
2. Read `docs/README.md`, then `docs/project-truth.md`.
3. Read the feature specification under `docs/features/` that owns the area.
4. Read the related architecture documents and ADRs.
5. Read `docs/engineering/known-issues.md` and
   `docs/engineering/open-questions.md`.
6. Inspect the real source, migrations, tests and `git diff`.

Write down the task in this shape before touching code:

```text
Outcome      the intended observable behaviour
Requirements the accepted requirements this must satisfy
Existing     what the code does today, with file references
Constraints  invariants, contracts and dependencies that must hold
Code         the files that must change
Docs         the documents that must change
Verify       the commands that prove it works
Conflicts    any accepted requirement or open decision this touches
```

## 2. Risks

Classify the change before implementing it:

| Class                | Examples                          | Required depth                       |
| -------------------- | --------------------------------- | ------------------------------------ |
| Routine              | refactor, docs, tests             | implement + targeted test + review    |
| Contract-affecting   | new endpoint, changed DTO         | + full test suite + API docs + ADR    |
| Security-sensitive   | auth, tokens, authorization       | + security engineer + security tests  |
| Data-changing        | migration, schema                 | + database engineer + migration review|
| Cross-module         | refactor across modules           | + architecture review before coding  |

A security-sensitive, data-changing or cross-module change must have the
design agreed by the architect **before** implementation starts.

## 3. Implement in bounded stages

- One stage = one coherent, testable increment.
- Run the targeted test after each stage. Do not accumulate untested changes.
- Keep the working tree buildable at every stage.

## 4. Verify

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
TEST_DATABASE_URL=... cargo test --all-targets
```

Record the real output. Never claim a check passed that you did not run.

## 5. Review

Ask the `code-reviewer` subagent to inspect the diff. It is read-only. Fix
every finding that is actionable, then re-run the checks — a fix can break a
test.

## 6. Quality gate

Ask the `quality-gate` subagent to judge the result against the evidence. It
returns `PASS` or `FAIL`. A `FAIL` must be resolved and re-checked.

## 7. Document

Follow the `atom-docs` skill. A change is not complete until its documentation
is updated or you have explicitly recorded why none was needed.

## 8. Report

```text
Outcome      what now works
Changed      <file paths>
Verified     <command> -> <result>
Docs         <documents updated, or "none needed: <reason>">
Open         <limitations and follow-ups>
```

## Prohibited

- Never fabricate a test result or a verification output.
- Never weaken a check to make it pass. Fix the code or record the blocker.
- Never run a destructive Git command against the user's work.
- Never mark a feature `Verified` without a recorded passing check.
- Never silently change accepted product behaviour. Record the discrepancy.
