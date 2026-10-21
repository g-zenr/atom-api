# Review record: `ATOM-REVIEW-YYYYMMDD-NNN`

> **Date:** `<YYYY-MM-DD>` (UTC)
> **Reviewer:** `<role>` — the role, never a personal name
> **Scope:** `<the commit, branch or file set that was reviewed>`
> **Verdict:** `<APPROVE | APPROVE WITH COMMENTS | REQUEST CHANGES>`

One entry of the review log. The identifier is
`ATOM-REVIEW-YYYYMMDD-NNN`, where `NNN` is a three-digit sequence that has not
been used before.

The reviewer is **read-only**: it inspects the change and never edits the
implementation. The verdict is `APPROVE`, `APPROVE WITH COMMENTS` or
`REQUEST CHANGES`.

Insert the finished entry at the top of
[`../engineering/review-log.md`](../engineering/review-log.md).

---

## What was reviewed

*The change under review, and the context the reviewer was given: the accepted
requirements, the feature documents, the API contract and the change-log
entry. A review that does not state its scope cannot be repeated.*

## Verdict

*One paragraph: the verdict, the reason, and the condition under which it
would change. `APPROVE WITH COMMENTS` means the change may proceed and the
comments must be dealt with separately. `REQUEST CHANGES` means the change may
not proceed until the required fixes are in place.*

## Findings

*Each finding, ordered by severity: `Blocker`, `Major`, `Minor`, `Note`. Every
finding names a file and a location, says what breaks, and proposes the
concrete fix. A finding without a fix is an observation, not a finding.*

| ID | Severity | Location | What breaks | The fix |
| -- | -------- | -------- | ------------ | ------- |
| `R-01` | `<severity>` | `<path>` | `<the defect and its impact>` | `<the concrete fix>` |

## Required fixes

*The subset of the findings that must be fixed before the change is complete.
Everything here must appear again in the resolution table, with the evidence
that it was fixed.*

| Finding | Must be fixed | Owner |
| ------- | ------------- | ----- |

## Resolution

*What was done about each required fix: the change that carried it, and the
issue it was recorded in when it was accepted instead of fixed.*

| Finding | Resolution | Change |
| ------- | ---------- | ------ |

## Re-review evidence

*The proof that the fixes hold. A finding is closed only when a check was run
again and its output is recorded here.*

| Finding | Check re-run | Result | Re-inspected by |
| ------- | ------------ | ------ | --------------- |

## Documentation findings

*Where the documentation did not match the implementation: a claim that is not
true, a document that is missing, a link that does not resolve. The
documentation checker is a tool, not a judge; a reviewer reads the prose.*

| Location | Finding |
| -------- | ------- |

## Limitations

*What the reviewer could not verify, and why: a check that could not be run, a
part of the system that was out of scope, a claim that rests on the author's
word. This section is where "not verified" is recorded, so that the gap is
visible rather than assumed away.*

## Links

*The change under review, the related ADRs, the known issues and the open
questions.*
