# Phase 2 run — personal workspace

> **Status:** In progress (run record)
> **Last updated:** 2026-10-24
> **Owner:** maintained by the orchestrator

This folder is the durable home of the ATOM Phase 2 build: reimbursements, job
application tracking and local machine learning on top of the existing notes
and tasks. It exists so that any session — tomorrow, next week, or after a
crash — can resume without the conversation that produced it.

## The files

| File | What it is | Read it when |
| ---- | ---------- | ------------ |
| [prompt.md](prompt.md) | The self-healing daily prompt. One day per paste. | You are starting a session |
| [plan.md](plan.md) | Scope, assumptions, data model, ML design and the six-day wave table | You need to know what and why |
| [ledger.md](ledger.md) | The live state: baseline, queue, evidence, failures, checkpoints, resume | Always, first |

These four files are created untracked. Commit them before the first preflight:
the run folder must travel with the repository, and the preflight treats a dirty
tree as a signal to stop and look rather than to plough on.

## How to resume

1. Open [ledger.md](ledger.md). Read `## Resume` and `## Queue`.
2. Take the first queue item whose state is not `done`.
3. Paste [prompt.md](prompt.md) into a fresh session in the repository root,
   replacing `DAY=<n>` with the day that item belongs to.
4. Let the preflight run. If it refuses because the tree is dirty or a process
   is running, do not force it: that is the guard working.
5. When the session ends, the ledger is the handover. Commit it.

## The order of the work

Phase 1 is not closed yet. The close-out (Day 0) is a separate wave: the
documentation feature, the shutdown signal test and the container image were
being written when this folder was created. Do not start Day 1 until the
close-out is committed and its six gates are green on a clean tree.

| Day | Wave | Output |
| --- | ---- | ------ |
| 0 | Close Phase 1 | The remediation leftovers, verified and merged |
| 1 | Product intake | Accepted requirements, feature specs, ADRs, schema — documents only |
| 2 | Reimbursements | Migration, module, endpoints, tests, docs |
| 3 | Job applications | Migration, module, endpoints, tests, docs |
| 4 | Local ML | ADR, embeddings, hybrid search, tests, evaluation |
| 5 | Hardening | Contract tests, ownership sweep, performance, frontend handover |

The detail of each day is in [plan.md](plan.md) §7.

## The rules that do not change

- [AGENTS.md](../../../../AGENTS.md) is binding. The short version:
  ownership in SQL, `404` never `403`, no secret in a log, migrations
  append-only, no new dependency without a recorded reason, no lint suppressed.
- A feature is `Verified` only when a recorded check passes. Never before.
- A public contract change means OpenAPI, `docs/api/` and the feature documents.
- No agent edits a file another agent owns in the same wave.

## Where the rest of the knowledge lives

- [The documentation index](../../../README.md)
- [The product requirements](../../../project-truth.md)
- [The roadmap](../../../roadmap.md)
- [The remediation backlog](../../../audit/remediation-backlog.md)
- [Known issues](../../known-issues.md)
- [Open questions](../../open-questions.md)
