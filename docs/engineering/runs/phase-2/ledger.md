# Phase 2 run — ledger

> **Status:** In progress (run state)
> **Last updated:** 2026-10-24 03:15 (UTC+8)
> **Owner:** maintained by the orchestrator

This file is the only source of truth for resuming the run. It is updated after
every chain, in the same commit as the chain. A chat transcript is not a
resume point; this file is.

---

## Owner decisions (2026-10-24, before the first run)

Recorded so the run never has to ask again. These override the defaults in the
plan and the prompt where they conflict; they do not change `AGENTS.md`.

| # | Decision | Consequence |
| - | -------- | ----------- |
| O1 | Assumptions A1–A6 in [plan.md](plan.md) §3 were **not explicitly confirmed**; the owner said to proceed and see what can be finished. | They are recorded as **provisional defaults** and marked as such in every document Day 1 writes. Day 1 must not present them as owner-approved. If the owner revises one, the affected spec is amended in a new change, not silently. |
| O2 | Git: **push directly to `develop`** for now, no pull requests. | Each day works on a day branch for rollback points, then merges into `develop` and pushes at the checkpoint. Never force-push. CI runs on the push to `develop`. The deviation from `docs/engineering/git-workflow.md` is temporary and recorded in the change log. |
| O3 | Range: **as far as it gets**, through Day 5. | Chains that do not fit are parked with evidence. Quality gates are not traded for distance. |
| O4 | Disk cleanup authorised. | Build artifacts and unused Docker images may be deleted. Current free space after cleanup: see Evidence. |
| O5 | The concurrent session was **not** identified by the owner. | The runner never kills it. It waits for ten quiet minutes, and aborts if the folder is still busy after sixty. |
| O6 | **Tech stack: Rust only.** The owner does not want another language runtime. | ML inference must be pure Rust. **Burn is the chosen framework** (owner, 2026-10-24). No Ollama, no Python sidecar, no external model server. Model weights are local files, not a service. PowerShell, `gh` and Docker remain developer tooling. |
| O7 | **n8n is a later integration**, not Phase 2. | A separate Node service that calls the API; the API never depends on it and it never sits inside the data path of a private record. It stays `Proposed` in `docs/roadmap.md`. It does not violate O6 because it is not part of the API runtime. |
| O8 | **Full authority granted** (2026-10-24, before sleep): proceed without approval, API only, no frontend, attempt every day. | The runner passes `--auto`. The agent must not pause for confirmation, and must not ask for permission for a tool it already has. Full authority is not permission to fake a result: the anti-cheat rules and the stop conditions still hold. A day that cannot be finished is parked with evidence; the runner then stops rather than build the next day on a broken base. |

---

## Baseline

Recorded on 2026-10-24 at 03:15, before Day 1. Re-verify before trusting it.

| Fact | Value |
| ---- | ----- |
| HEAD | `6e8a0c4` — `origin/develop` was the same commit at the time |
| Branch | `chore/post-remediation-followups` |
| Phase 1 backlog | 83 of 83 findings accounted for: 78 `verified`, 5 `deferred` (`F-23`, `F-44`, `F-45`, `F-46`, `F-49`), 11 retained |
| Last change-log entry | `ATOM-CHANGE-20261023-001` |
| Last full test run | 2026-10-22: 177 tests (111 unit, 29 auth, 12 notes, 25 projects/tasks) |
| PostgreSQL | 18, accepting connections on 127.0.0.1:5432; `atom_test` exists |
| `gh` | authenticated as `g-zenr`, scopes include `repo` and `workflow` |
| Docker | 29.2.1 available |
| Rust | 1.92.0 |
| Disk | 5.7 GB free on `C:` at the time — keep above 3 GB before a release build |
| pgvector | extension `vector` 0.8.1 is available, but the `atom` role is **not** a superuser and `CREATE EXTENSION vector` is denied locally |

Gates at the time of the snapshot, on a tree another session was editing:

| Gate | Result |
| ---- | ------ |
| `cargo check --no-default-features --all-targets --locked` | exit 0 |
| `cargo clippy --all-targets --all-features --locked -- -D warnings` | exit 0 |
| `cargo fmt --all -- --check` | exit 1 — only `tests/shutdown.rs`, unformatted |
| `cargo test --all-targets --locked` | not run: the tree was in flux |
| `atom-docs` / `--strict` | not run at the snapshot |

---

## Queue

Scope addition, 2026-10-24: a complete database seeder (`atom-seed`) is part of
Phase 2. It is specified in [plan.md](plan.md) §8 and tracked by the S-rows
below. Every chain that adds a table extends it in the same chain.

| Item | Wave | State | Commit | Notes |
| ---- | ---- | ----- | ------ | ----- |
| D0 | Close Phase 1 | **partially done** | `71600cb` | PR #5 merged the `docs` feature, the container image, the shutdown test, `020` and `027`. Remaining: `F-23` (021), `F-49` (023), the fate of `docs/audit/handoff.md`, and the `config/README.md` line-ending dirt. |
| D1 | Product intake | pending | — | Documents only. Blocks D2. Specifies the seeder in `docs/database/seeding.md`. |
| D2.S1 | Seeder foundation | pending | — | `atom-seed` for users, notes, projects and tasks. Guards, determinism, `--reset`, tests, docs. No new dependency. |
| D2.R1 | Reimbursements — schema and types | pending | — | |
| D2.R2 | Claim CRUD | pending | — | |
| D2.R3 | Items and the total invariant | pending | — | |
| D2.R4 | Lifecycle actions | pending | — | |
| D2.R5 | Flow and contract tests | pending | — | Uses the second seeded account for the ownership sweep. |
| D2.R6 | Close: review, gate, commit | pending | — | Also D2.S2: extend the seeder with claims. |
| D3 | Job applications | pending | — | Blocked by D2. |
| D3.S3 | Seeder: companies and applications | pending | — | Lands with the Day 3 close. |
| D4 | Local ML | pending | — | Blocked by D3; needs the embedding provider decision from Day 1. |
| D4.S4 | Seeder: embeddings | pending | — | `--with-embeddings` uses the deterministic embedder, never a network call. |
| D5 | Hardening and handover | pending | — | Blocked by D4. |
| D5.S5 | Seeder: CI smoke and final proof | pending | — | Runs the seeder against the CI database and asserts the counts. |

A day is one paste of [prompt.md](prompt.md). A chain is one row above.

---

## Contracts

Locked contracts are appended here, newest last, and pasted verbatim into every
agent brief for that chain. Empty until D2.R1.

---

## Evidence

Only commands actually run, with their real result.

| Date | Command | Result |
| ---- | ------- | ------ |
| 2026-10-24 03:03 | `cargo check --no-default-features --all-targets --locked` | exit 0 |
| 2026-10-24 03:03 | `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| 2026-10-24 03:03 | `cargo fmt --all -- --check` | exit 1, `tests/shutdown.rs` |
| 2026-10-24 03:09 | `psql -c "create extension vector"` as `atom` | `permission denied` — must be superuser |
| 2026-10-24 03:09 | `pg_available_extensions` | `vector 0.8.1`, `pg_trgm 1.6`, `pgcrypto 1.4`, `unaccent 1.1` |
| 2026-10-24 03:11 | `docker --version`, `gh auth status`, `pg_isready` | 29.2.1; logged in as `g-zenr`; accepting connections |

| 2026-10-24 03:58 | `git log --oneline` | PR #5 merged as `71600cb`: the `docs` feature, the container image, the shutdown test, `020`, `027`. `develop` was two run-folder commits ahead of origin. |
| 2026-10-24 03:53 | `cargo run --bin atom-docs` | exit 0, 0 warnings; `--strict` exit 0, 0 warnings |
| 2026-10-24 03:52 | `python` UTF-8 check of the run folder | valid, 0 replacement characters |
| 2026-10-24 03:52 | disk cleanup | removed incremental, PDB and release artifacts: 1.3 GB free to 9.27 GB |

---

## Failures

One line per repair, so the next chain does not repeat it.

```
FAILURE: <command> -> <error> | ROOT CAUSE: <one line> | FIX: <one line> | TEST: <that now passes>
```

Empty. Nothing has been repaired by this run yet.

---

## Checkpoints

One scorecard table per chain, per the prompt's §7. Empty.

---

## Parked

| Item | Blocker | Next step |
| ---- | ------- | --------- |
| pgvector | The `atom` role is not a superuser locally; CI's `atom` is. A migration with `CREATE EXTENSION` would pass CI and fail on this machine. | Default design avoids the extension (see [plan.md](plan.md) §6). If pgvector is wanted, run `CREATE EXTENSION vector` as `postgres`, document it as a deployment prerequisite, and add a preflight check. |
| Assumptions A1–A6 | They are product decisions, not engineering ones. | Confirm or correct them before Day 1 writes acceptance criteria. Unanswered assumptions become open questions and their behaviour is excluded from the specs. |
| `docs/audit/handoff.md` | Untracked, and stale: it says no pull request has been merged, while #1–#4 are. | Day 0 either updates and commits it or deletes it. |

---

## Resume

An overnight run was launched on 2026-10-24 to execute Days 0-5, stopping at the
first day that cannot be finished honestly. Its log is under `target/overnight/`
and the STOP sentinel is `STOP-RUN` in the repository root.

If the run stopped early, read `## Queue` and `## Parked` first, then paste
[prompt.md](prompt.md) with the first day whose state is not `done`. If it
reached Day 1, review the specifications before Day 2: A1-A6 are provisional
(owner decision O1) and the specs must say so. Day 2 starts with **S1**, the
seeder foundation, then **R1** from the locked contract in [plan.md](plan.md) §5.
