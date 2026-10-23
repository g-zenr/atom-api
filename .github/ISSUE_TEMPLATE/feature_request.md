---
name: Feature request
about: Propose a capability that is not yet accepted scope
title: ""
labels: ["enhancement"]
assignees: ""
---

> **Read this first.** A roadmap item is not agreed scope. Only
> [`docs/project-truth.md`](../docs/project-truth.md) and accepted ADRs define
> product behaviour, so a request here is a **proposal**, not a commitment.
> See [`docs/roadmap.md`](../docs/roadmap.md) for what is already planned.

## The problem

What can you not do today, and why does it matter? Describe the problem rather
than a specific implementation.

## Proposed capability

What you would like the API to do.

## Which requirement would it serve?

- Existing requirement it extends: [`docs/project-truth.md`](../docs/project-truth.md) §…
- Or: a new requirement, which needs an ADR before it is implemented

## Scope check

- Does it change a published API contract? If so, that is a breaking change and
  needs its own discussion.
- Does it need a schema change? If so, it needs a migration that is forward-only.
- Does it touch authentication, authorization or data isolation? If so, say how
  ownership will be enforced in SQL.
- Does it need a new setting? If so, it needs the file, the typed field, the
  loader, the validation, `.env.example`, the reference table and tests — see
  `AGENTS.md` §4 rule 11.

## Alternatives considered

What else could solve the same problem, and why is the proposal better?

## What it is not

Explicitly note anything that could be misread as part of the request, so the
scope stays small.
