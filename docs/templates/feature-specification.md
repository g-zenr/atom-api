# Feature specification: `<feature name>`

> **Status:** `<Proposed | Accepted | In progress | Implemented | Verified | Deprecated | Blocked>`
> **Last updated:** `<YYYY-MM-DD>`
> **Owner:** `<role>`

Replace every `<placeholder>` before the document is committed. Delete no
section: a section that does not apply says `Not applicable: <reason>`, so that
the question it asks is visibly answered rather than silently dropped.

The status words are the seven defined in
[`../../AGENTS.md`](../../AGENTS.md) §6. Do not invent another one. A feature is
never `Verified` because its code exists.

---

## 1. Purpose

*One paragraph: the problem this feature solves and for whom. If this cannot
be written without naming an implementation, the purpose is not clear yet.*

## 2. Scope

*The observable behaviour this feature delivers. A list of capabilities, each
one stated so that it can be tested.*

## 3. Non-goals

*What this feature deliberately does not do, and why. A non-goal protects a
later contributor from "improving" the feature into something that was
rejected on purpose.*

## 4. Requirements

*One row per accepted requirement. Every row must trace to a requirement in
[`../project-truth.md`](../project-truth.md); a requirement that is not
accepted yet belongs in the roadmap, not here.*

| ID | Requirement | Accepted in | Priority | Notes |
| -- | ----------- | ----------- | -------- | ----- |
| `R-1` | `<requirement>` | `project-truth.md` §4.x | Must | |

## 5. User-facing behaviour

*What a client sees. The concrete rules: the status codes, the error codes,
the pagination convention, the partial-update semantics, the identifiers and
the timestamps. Refer to [`../api/conventions.md`](../api/conventions.md)
rather than restating it.*

## 6. Acceptance criteria

*A checklist that can be verified. Each item names the check that proves it —
a test, a recorded command, or an inspection.*

- [ ] `<criterion>` — proven by `<test name or command>`
- [ ] `<criterion>` — proven by `<test name or command>`

## 7. Dependencies

*What this feature needs, and what it constrains. Name the accepted features
it builds on, and the ones that must stay independent of it.*

| Dependency | Direction | Note |
| ---------- | --------- | ---- |

## 8. Security and privacy

*What is protected, how, and what a hostile caller must not be able to do. Name
the control and where it lives. If the feature touches user data, the
ownership predicate and the inaccessible-resource behaviour are stated here.
See [`../security/secure-development.md`](../security/secure-development.md).*

## 9. Data and schema

*The tables or columns the feature adds or changes, and the invariants that
belong in the database. Migrations are append-only: describe the new
migration, never an edit to an applied one.*

## 10. Open questions

*Every decision this feature waits for, with a link to its entry in
[`../engineering/open-questions.md`](../engineering/open-questions.md). If
there are none, say so.*

## 11. Related documents

| Document | Relationship |
| -------- | ------------ |
| [`../architecture/`](../architecture/) | How the feature is built. |
| [`../api/`](../api/) | The public contract. |
| [`../database/schema.md`](../database/schema.md) | The schema. |
| [`../testing/strategy.md`](../testing/strategy.md) | How the feature is tested. |
| [`../engineering/change-log.md`](../engineering/change-log.md) | The changes that built it. |
