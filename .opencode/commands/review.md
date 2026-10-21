---
description: Review the current changes for correctness, security, regressions and documentation accuracy.
agent: code-reviewer
---

Review the current uncommitted changes in this repository.

Context for the review:

- Accepted product requirements: `docs/project-truth.md`
- Feature specifications: `docs/features/`
- API contracts: `docs/api/` and the OpenAPI document served at
  `/api/v1/openapi.json`
- Architecture and decisions: `docs/architecture/` and `docs/decisions/`

Focus on: correctness against the specification, authorization and ownership
predicates, input validation, secret leakage, SQL parameterisation, Rust
idioms, migration safety, API compatibility and documentation accuracy.

Report findings ordered by severity, each with a file and line reference, why
it breaks, and the concrete fix. End with a verdict: APPROVE, APPROVE WITH
COMMENTS, or REQUEST CHANGES.

Do not modify any file.
