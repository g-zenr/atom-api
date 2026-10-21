# ATOM verification policy

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-21
> **Owner:** maintained by the `docs` agent

This document says what must pass before a change is considered complete, what
"verified" means, and how a result is reported. It is the rule that
[`../../AGENTS.md`](../../AGENTS.md) §7 and the
[`atom-delivery` skill](../../.opencode/skills/atom-delivery/SKILL.md) refer
to.

The rule that governs everything else: **a result is only claimed when it was
observed.** A check that did not run is reported as *not run*, with the exact
command needed to run it.

---

## 1. The quality gates

Three commands are the quality gates. All three must pass before a change is
complete.

| # | Gate | Command | What it proves |
| - | ---- | ------- | -------------- |
| 1 | Format | `cargo fmt --all -- --check` | The tree is formatted the way `rustfmt` formats it. A difference is a failure. |
| 2 | Lint | `cargo clippy --all-targets --all-features -- -D warnings` | There is no lint finding anywhere in the library, the binaries, the tests and every feature combination. A warning is a failure, because `-D warnings` turns it into an error. |
| 3 | Test | `cargo test --all-targets` | The unit tests and the integration tests pass against a real PostgreSQL database. |

The test gate needs a disposable database:

```bash
# Unix
export TEST_DATABASE_URL=postgres://atom:atom@127.0.0.1:5432/atom_test
cargo test --all-targets
```

```powershell
# Windows PowerShell
$env:TEST_DATABASE_URL = "postgres://atom:atom@127.0.0.1:5432/atom_test"
cargo test --all-targets
```

The harness refuses to run against a database whose name does not contain
`test`. If the test gate cannot run, the change is not complete — see §4.

A fourth check is the documentation gate. It is required whenever a document
changed, and it is part of the review, not of the three code gates:

```bash
cargo run --bin atom-docs          # the same check: ./scripts/check-docs.sh
```

It verifies that every required document exists, that every relative link
resolves, that every feature status uses the defined vocabulary, that every
ADR has a status and a unique identifier, that every change identifier is
unique, and that every feature directory has its five documents.

`make check` runs gates 1 to 3 in order. `make docs-check` runs the
documentation gate.

## 2. What "verified" means

The status vocabulary is defined in [`../../AGENTS.md`](../../AGENTS.md) §6. For
testing, the distinction that matters is between `Implemented` and `Verified`:

| Status | Meaning |
| ------ | ------- |
| `Implemented` | The code exists and compiles. **Not** evidence that it behaves correctly. |
| `Verified` | Implemented **and** covered by a test or a recorded check that passes. |

Concretely, a feature is `Verified` only when all of the following are true:

1. The three quality gates passed on the current tree, and the output was
   recorded.
2. At least one test exercises the behaviour, and that test passes.
3. For a private endpoint, a cross-account isolation case exists and passes.
4. The documentation matches the code, and the documentation checker resolves
   every link.
5. An independent reviewer has inspected the change and raised no unresolved
   finding, or every finding has been fixed and re-verified.

A feature is never `Verified` because the code exists, because it compiles, or
because a reviewer said it looked right. The evidence is the test, and the
record of the test is the change log.

## 3. How much to run for which change

The change is classified before it is implemented, and the class decides the
depth. This is the risk table of the
[`atom-delivery` skill](../../.opencode/skills/atom-delivery/SKILL.md).

| Class | Examples | Required depth |
| ----- | -------- | -------------- |
| Routine | A refactor, a documentation change, a test | Implement, run the targeted test, run the three gates, review. |
| Contract-affecting | A new endpoint, a changed DTO | The above, plus the API documents and the OpenAPI annotations, plus an ADR if a decision was made. |
| Security-sensitive | Authentication, tokens, authorization | The above, plus the `auth-security` specialist, plus the security cases listed in [`../security/secure-development.md`](../security/secure-development.md) §3. |
| Data-changing | A migration, a schema change | The above, plus the `database` specialist, plus the migration applied to a clean database. |
| Cross-module | A refactor that crosses module boundaries | The above, plus the design agreed by the `architect` **before** the code is written. |

The three gates always run. A change may never skip the test gate because the
change "is only documentation".

## 4. Reporting a check that could not run

A check that could not be executed is reported as **not run**. It is never
reported as passing, and never silently omitted.

The report says three things:

1. **Which check** did not run. `cargo test --all-targets`.
2. **Why** it did not run. `TEST_DATABASE_URL` was not set, and no PostgreSQL
   instance was reachable at the time of the run.
3. **What is needed to run it.** The exact command, including the environment
   variable:

   ```text
   $env:TEST_DATABASE_URL = "postgres://atom:atom@127.0.0.1:5432/atom_test"
   cargo test --all-targets
   ```

The same rule applies to a gate that ran but did not complete: a truncated run,
a timeout or a cancelled run is *not run*, not *failed* and not *passed*.

If a gate fails, the failure is fixed or reported as a blocker with the exact
error. A failing gate is never "accepted for now" without an entry in
[`../engineering/known-issues.md`](../engineering/known-issues.md) that states
the impact and the workaround.

## 5. Re-verification after a change

A fix is a change. The same rules apply to it.

- After **any** code change, the three gates are run again. A fix that was not
  re-verified is an unverified fix.
- When a fix addresses a finding, the check that would have caught the finding
  is run again and its output is recorded **in the same change-log entry**.
- A review finding that is closed without a new passing check stays open. The
  resolution and its evidence are recorded in
  [`../engineering/review-log.md`](../engineering/review-log.md) under
  re-review evidence.
- A change to the documentation alone still runs the documentation gate, and
  still needs the reviewer to confirm that the documentation matches the code.

A concrete consequence: changing a test to make it pass is a re-verification
of nothing. The gate that matters is the full suite, and the diff of the test
is part of what the reviewer inspects.

## 6. The review and quality-gate sequence

```text
implement  ->  run the three gates  ->  record the real output
          ->  code-reviewer (read-only)  ->  fix the findings
          ->  run the three gates again  ->  record the new output
          ->  quality-gate (read-only)  ->  PASS or FAIL
          ->  document  ->  report
```

- The **code reviewer** is independent of the agent that wrote the code and is
  **read-only**: it inspects the diff, runs read-only checks and reports
  findings. It never edits the implementation. Its findings are recorded in
  [`../engineering/review-log.md`](../engineering/review-log.md).
- The **quality gate** is also read-only. It judges the *evidence*, not the
  summary: the commands that were run, their real output, and whether the
  documentation matches the implementation. It returns exactly `PASS` or
  `FAIL`.
- A `FAIL` is resolved and re-checked. The resolution and its evidence are
  recorded, not just the fact that it was addressed.
- The gate does not re-run the work for the implementation agent; it reports
  what it observed.

## 7. The reporting format

Every task ends with the format the orchestrator agent is required to use. It
is short, and every line is either a fact or an explicit gap.

```text
Outcome      <what now works, or why it does not>
Changed      <file paths>
Verified     <command> -> <result>       (only commands actually run)
Docs         <documents updated, or "none needed: <reason>">
Open         <limitations, blockers, follow-ups>
```

Rules for the `Verified` line:

- One line per command that was actually run, with its real result.
- A command that was not run is either absent or explicitly written as
  `not run: <reason>`. It is never written as if it had passed.
- A test count is written as it was observed, for example
  `cargo test --all-targets -> passed, 69 tests (39 unit, 13 auth, 8 notes, 9 projects/tasks)`.
- A partially successful run says which part failed.

Rules for the `Docs` line:

- It names the documents that changed, or states why no document needed to
  change. "No documentation change was needed" is a valid answer, but it must
  be a reason, not a shrug.

Rules for the `Open` line:

- It names every limitation, blocker and follow-up, and points at the entry in
  [`../engineering/known-issues.md`](../engineering/known-issues.md) or
  [`../engineering/open-questions.md`](../engineering/open-questions.md) that
  carries it.

A long-standing record of this format is in
[`../engineering/change-log.md`](../engineering/change-log.md), and the
per-review form is in
[`../engineering/review-log.md`](../engineering/review-log.md).
