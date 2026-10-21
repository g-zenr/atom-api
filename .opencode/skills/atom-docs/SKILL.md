---
name: Atom Docs
description: Use before and after any implementation work that touches behaviour, contracts, schema or decisions. Defines the mandatory documentation intake and update protocol for the ATOM knowledge base.
---

# Documentation protocol

Documentation is part of the product. Every agent follows this protocol.

## Before implementation

1. Read `AGENTS.md` and the applicable repository instructions.
2. Read `docs/README.md` — the index and the authority order.
3. Read `docs/project-truth.md` — the accepted product requirements.
4. Read the relevant roadmap and `docs/engineering/implementation-status.md`.
5. Read the applicable feature specifications under `docs/features/`.
6. Read the related architecture documents and ADRs.
7. Read the relevant changelog, review log, known issues and open questions.
8. Inspect the actual source code, migrations, tests and Git diff.
9. Check whether the task conflicts with an accepted requirement or an
   unresolved decision.
10. Identify which documentation must change alongside the code.

Read the canonical overview first, then retrieve what the task needs. Do not
dump every document into context for a small change. For broad architectural
work, do the wider review.

If the environment offers a document search tool, use it to locate material,
but verify important claims against the canonical files and the implementation.

## Before making changes

Produce this summary — internally, or in the delegation brief when the work
depends on shared requirements:

```text
Outcome      intended observable behaviour
Requirements accepted requirements involved
Existing     current implementation, with file references
Constraints  invariants and contracts that must hold
Code         required code changes
Docs         required documentation changes
Verify       tests and verification criteria
Conflicts    accepted requirements or open decisions affected
```

## After implementation

1. Update the affected feature's `status.md`.
2. Update the architecture documents when structure, invariants, the request
   lifecycle or a security boundary changed.
3. Add or update an ADR when a decision materially affects structure,
   security, compatibility, persistence or long-term maintainability.
4. Append a chronological entry to `docs/engineering/change-log.md`.
5. Record review findings and their resolution in
   `docs/engineering/review-log.md`.
6. Update `docs/engineering/implementation-status.md` and
   `docs/engineering/known-issues.md`.
7. Update the OpenAPI annotations and `docs/api/` when a contract changed.
8. Verify that every documentation claim matches the implementation.
9. Run the documentation checker.
10. Include the documentation changes in the final review.

**A code change is not complete until its documentation is updated, or the
agent has explicitly explained why no documentation change was necessary.** The
completion report states which.

## Handling a conflict

When the code, the documentation and `docs/project-truth.md` disagree:

1. Identify both conflicting sources and quote them.
2. Explain the impact.
3. Check whether an accepted ADR already resolves it.
4. If it is unresolved, record it as a discrepancy in
   `docs/engineering/known-issues.md` (or `open-questions.md` when a product
   decision is needed).
5. Continue with the work that does not depend on the answer.
6. Ask the human only when a material decision genuinely cannot be made safely.

Never rewrite accepted requirements to match the code. Never let a
documentation-only change conceal an implementation failure.

## Reference

| Document                                          | Use it for                                     |
| ------------------------------------------------- | ---------------------------------------------- |
| `docs/templates/feature-specification.md`          | a new or changed feature specification        |
| `docs/templates/adr.md`                            | a material decision                            |
| `docs/templates/change-record.md`                 | a change-log entry                             |
| `docs/templates/review-record.md`                 | a review-log entry                             |
| `docs/templates/verification-record.md`           | a verification record                          |
| `docs/engineering/implementation-status.md`       | the current verified state of every feature    |
| `docs/engineering/known-issues.md`                | confirmed defects and limitations              |
| `docs/engineering/open-questions.md`              | unresolved product or architecture decisions   |

## Identifier conventions

- Change entries: `ATOM-CHANGE-YYYYMMDD-NNN`.
- ADRs: `ADR-NNNN-short-slug`, never reused, never deleted.
- Review entries: `ATOM-REVIEW-YYYYMMDD-NNN`.
- Verification records: `ATOM-VERIFY-YYYYMMDD-NNN`.
