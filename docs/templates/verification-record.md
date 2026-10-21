# Verification record: `ATOM-VERIFY-YYYYMMDD-NNN`

> **Date:** `<YYYY-MM-DD>` (UTC)
> **Verifier:** `<role>` — the role, never a personal name
> **Subject:** `<what was verified: the change, the feature, the release>`
> **Result:** `<PASS | FAIL>`

One entry of the verification record. The identifier is
`ATOM-VERIFY-YYYYMMDD-NNN`, where `NNN` is a three-digit sequence that has
not been used before.

A verification record states what was checked, how it was checked, what the
real output was, and what could **not** be checked. A check that did not run
is recorded as *not run*, never as a pass. See
[`../testing/verification-policy.md`](../testing/verification-policy.md).

---

## Scope

*What was verified, and what was explicitly out of scope. Name the change or
the feature and the commit or the state of the tree it was verified at.*

## Environment

*The environment the checks ran in, in the terms that affect the result: the
operating system, the Rust toolchain, the database and its version, and the
configuration that was in force. No absolute paths, no credentials, no
personal names.*

| Item | Value |
| ---- | ----- |
| Operating system | `<value>` |
| Rust toolchain | `<value>` |
| Database | `<name and version>` |
| Configuration | `<the configuration that matters, without any secret>` |

## Checks and results

*The commands that were actually run, and their real output. A check that was
not run says so.*

| # | Command | Expected | Actual | Result |
| - | ------- | -------- | ------ | ------ |
| 1 | `cargo fmt --all -- --check` | No differences | `<output>` | `<pass or fail>` |
| 2 | `cargo clippy --all-targets --all-features -- -D warnings` | No findings | `<output>` | `<pass or fail>` |
| 3 | `cargo test --all-targets` | Every test passes | `<counts and the failures, if any>` | `<pass or fail>` |
| 4 | `cargo run --bin atom-docs` | No error | `<output>` | `<pass or fail>` |

## Manual verification

*What was exercised by hand, the steps, and what was observed. A manual check
belongs here, with enough precision that it can be repeated.*

| Step | Expected | Observed |
| ---- | -------- | -------- |

## What was not verified, and why

*The gaps. This section is the most important one: it is the difference
between "verified" and "believed to work". Each item names the check that
would close it.*

| Item | Why it was not verified | The check that would close it |
| ---- | ----------------------- | ---------------------------- |

## Result

*`PASS` when every required check ran and passed, and the gaps above are
acceptable and recorded. `FAIL` when a required check failed, when a required
check could not run, or when a gap makes the result unreliable. State the
condition that would change the verdict.*

## Links

*The change, the review, the feature documents, the known issues and the open
questions.*
