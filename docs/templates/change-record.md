# Change record: `ATOM-CHANGE-YYYYMMDD-NNN`

> **Date:** `<YYYY-MM-DD>` (UTC)
> **Type:** `<Foundation | Feature | Fix | Refactor | Documentation | Dependency | Migration>`
> **Author:** `<role>` — the role, never a personal name
> **Status:** `<Complete | In progress | Reverted>`

One entry of the change log. The identifier is `ATOM-CHANGE-YYYYMMDD-NNN`,
where `NNN` is a three-digit sequence that has not been used before. The
change log is append-only: an entry is never rewritten, and an inaccurate
statement is corrected by adding a marked **Correction** at the end of the
entry.

Insert the finished entry at the top of
[`../engineering/change-log.md`](../engineering/change-log.md), under its
`## Entries` heading.

---

## Summary

*What changed, in three or four sentences. A reader who reads only this
section should know what the change does.*

## Reason

*Why the change was made. The problem, the requirement, or the decision it
addresses. A change with no reason is a change with no contract.*

## Affected features and files

*What the change touches. Every file that a reviewer must read, and the
feature whose status it changes.*

| Feature | Area | Files |
| ------- | ---- | ----- |
| `<feature>` | `<area>` | `<path>`, `<path>` |

## Requirements and decisions referenced

*Every accepted requirement, ADR and known issue the change satisfies or
touches. If the change contradicts an accepted requirement, it does not go in
this form — it is raised as a discrepancy first.*

| Reference | Subject |
| --------- | ------- |
| [`../project-truth.md`](../project-truth.md) §4.x | `<requirement>` |
| [`../decisions/ADR-NNNN-*.md`](../decisions/) | `<decision>` |

## Architectural impact

*The structural effect: the modules, the boundaries, the invariants, the
request lifecycle, the schema. `None.` is a valid answer when the change is
confined to one module — but say why it is confined.*

## Compatibility impact

*What a client could notice: a changed response shape, a changed status code,
a changed error code, a changed default, a changed configuration key. A new
feature that adds to the API is compatible; a change that removes or reshapes
is not, and it needs a migration note.*

## Security implications

*What the change does to the security boundary: the controls added, removed or
weakened, the trust boundaries it touches, the new inputs it accepts and how
they are validated. `None.` is a valid answer when the change cannot reach the
boundary — but say why.*

## Checks and results

*The commands that were actually run, and their real output. A check that did
not run is recorded as `Not run` with the reason and the exact command needed.
A check is never reported as passing that was not observed.*

| Command | Result |
| ------- | ------ |
| `cargo fmt --all -- --check` | `<result>` |
| `cargo clippy --all-targets --all-features -- -D warnings` | `<result>` |
| `cargo test --all-targets` | `<result>` |

## Manual verification

*What was exercised by hand, and what was observed. A manual check is a valid
check when it is recorded this precisely. `None.` is a valid answer.*

## Review findings

*The findings of the independent review, and how each one was resolved. The
full record is in [`../engineering/review-log.md`](../engineering/review-log.md);
this section summarises it. `No independent review has been performed on this
change set` is a valid answer, and it is the honest one when no reviewer has
run.*

| Finding | Severity | Resolution | Re-verified by |
| ------- | -------- | ---------- | -------------- |

## Documentation

*Which documents changed, or why none needed to. `No documentation change was
needed: <reason>` is a valid answer.*

## Links

*The related entries: the ADRs, the feature documents, the known issues, the
open questions, the review record.*
