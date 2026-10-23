# Release and promotion

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
> **Owner:** maintained by the engineering team

How a change moves from `develop` to a running environment, and what has to be
true at each gate. The branch and commit conventions that come before this are in
[`git-workflow.md`](git-workflow.md).

**Read the honesty clause first.** Phase 1 has **no deployment infrastructure**:
no container image, no orchestrator manifest, no UAT or PROD instance. What
follows is therefore a **process specification with two gates that are manual
today**, not an automation description. Nothing here should be read as a claim
that a deployment mechanism exists. `KI-006` records the missing artifact, and
[`implementation-status.md`](implementation-status.md) lists what is not built.

---

## 1. The lifecycle

```text
develop  ──►  DEV  ──►  UAT  ──►  PROD
   │           │         │         │
   │           │         │         └── release approval, post-deploy checks
   │           │         └──────────── acceptance of a specific commit
   │           └────────────────────── the six gates + smoke tests
   └────────────────────────────────── pull request merged
```

| Stage | Trigger | Gate |
| ----- | ------- | ---- |
| DEV | A pull request merges to `develop` | The six gates pass, or the merge does not happen |
| UAT | An authorised promotion of an identified commit | DEV is healthy; acceptance tests pass |
| PROD | An authorised release merge `develop` → `master` | UAT accepted that exact commit; rollback is understood |

**Promote an identity, not a branch tip.** Name the commit SHA. "The latest
`develop`" is not an identity: it can change between the approval and the
deployment.

## 2. DEV

DEV is the integration environment. It is what `develop` deploys to.

### Entry

- A pull request targeting `develop` passed the six gates and review, and was
  merged.
- The merge commit is the artifact being deployed.

### Verification

- `GET /health/live` answers `200` with `{"status":"live"}`.
- `GET /health/ready` answers `200`. It checks the database, so a failure here
  means the instance cannot serve, not that it is slow.
- The start-up log contains `database migrations are up to date` and
  `ATOM API is listening`.
- Configuration loaded: the start-up log names the environment but no secret. If
  the configuration is invalid the process exits before binding the socket, and
  the message names the setting and what was expected.
- The migrations applied match the migrations in the deployed commit.

### Failure

A failing gate means **no promotion**. Fix on a new task branch, merge through
`develop`, deploy again. Do not hand-edit a running instance: the next deploy
discards the edit and the difference is invisible until it matters.

## 3. UAT

UAT is the acceptance gate. It is where the product owner agrees that the
behaviour is what was asked for.

### Entry

- DEV has been verified for the **same commit** being promoted.
- The change's pull request lists what UAT should accept.

### What UAT must exercise

Not just "it starts". The flows that carry the product's promises:

| Area | Check |
| ---- | ----- |
| Registration and login | A new account can register; the password policy is enforced; a wrong password is `401` and does not reveal whether the account exists |
| Tokens | Login returns a token pair; `/auth/me` answers with the caller; refresh rotates; **a chain of rotations cannot outlive the original session deadline** |
| Authorization | A second account cannot read, update or delete the first account's note, project or task, and receives `404` — not `403` |
| Listings | Pagination honours the **configured** bounds; `search` is bounded; the reported total matches the filtered set |
| Errors | A malformed request returns the standard error envelope; a validation failure names the field |
| Rate limiting | Repeated failed logins eventually return `429` with `retry-after` |
| Configuration | The environment's settings are the ones that were intended — check a few visible ones (page size, rate limit) rather than trusting the file |

### Record

A UAT acceptance records: the commit SHA, the migration version applied, what was
tested, the result, who accepted it, and the date. A verbal "looks fine" is not a
record.

### Failure

Stop. Fix on a new task branch, merge through `develop`, redeploy to DEV, and
accept again. A failed UAT is never fixed by editing the UAT environment.

## 4. PROD

PROD is the controlled release.

### Entry

- UAT accepted **that exact commit**.
- A release merge from `develop` to `master` has been approved by a human.

### Before the merge

- Confirm the configuration the environment will read, **without printing secret
  values**: that `DATABASE_URL` and `APP_JWT_SECRET` are present in the platform
  secret store, and that `APP_ENV=production` (which selects the JSON log format
  and the production overlay).
- Confirm the migrations in the release are the ones that were tested in UAT.
- Know the rollback. **A migration is not assumed reversible.** The migrations
  are append-only; a release that changes the schema needs a forward-only plan
  (the new code tolerates both shapes for one release), not a `DROP`.

### The release

```text
git switch develop && git pull --ff-only
git switch master  && git pull --ff-only
git merge --no-ff develop
git push origin master
```

Then tag it, so the release has a name:

```text
git tag -a v<major>.<minor>.<patch> -m "release <version>"
git push origin v<major>.<minor>.<patch>
```

### After the release

- `/health/live` and `/health/ready` answer `200`.
- The critical path works: register or log in, create a note, list it.
- Watch errors, latency, authentication failures and `429` rates for a period
  proportional to the change's risk.
- Record the version, the commit SHA, the migration version, the approver, the
  time, the verification result and any incident.

### If it goes wrong

Stop the rollout. Roll back by redeploying the previous tagged release — the
previous **code** is always deployable because migrations are forward-only and a
release keeps the previous shape working for one cycle. If the schema moved in a
way the old code cannot read, the recovery is a forward fix, not a rollback, and
that must be decided by a human at the time.

## 5. Environment configuration

| Environment | `APP_ENV` | Configuration | Secrets |
| ----------- | --------- | ------------- | ------- |
| Local | `development` | The committed files | `.env`, git-ignored |
| DEV | `development` or `production` — as the team decides, but stated | The committed files plus the deployment's overlay | The platform secret store |
| UAT | as DEV | The committed files plus the UAT overlay | The UAT secret store, **never** a copy of production's |
| PROD | `production` | The committed files plus the production overlay | The production secret store |

Rules that hold everywhere:

- **No secret in a file.** `DATABASE_URL` and `APP_JWT_SECRET` have no file
  layer at all — see [`../decisions/ADR-0006-configuration-source-of-truth.md`](../decisions/ADR-0006-configuration-source-of-truth.md).
- **No secret in a log.** The start-up path reports the error kind and the
  SQLSTATE, never the driver message.
- **No shared database.** Each environment has its own. UAT never reads
  production data.
- **`APP_SHUTDOWN_GRACE_SECONDS`** bounds the drain after the process is asked to
  stop. It must stay below the time the orchestrator allows the process to stop,
  or the process is killed mid-drain — which is exactly what the setting exists
  to prevent.

## 6. What is not built yet

Recorded so that nobody mistakes this document for a description of working
automation.

| Missing | Consequence | Reference |
| ------- | ----------- | --------- |
| A container image | There is no immutable build artifact; a deployment is the source tree plus the toolchain | `KI-006` |
| An orchestrator manifest | Nothing declares replicas, probes or resource limits | `implementation-status.md` |
| A deployment pipeline | DEV, UAT and PROD are promoted by hand | this document |
| A run-time signal test for shutdown | The drain bound is verified by reading the code, not by a signal in a test | `ATOM-REVIEW-20261022-003` |
| Compile-time SQL verification | A renamed column fails at the first request, not at the build | `KI-004` |

The first of these is the one that matters most: without an image, "the same
artifact in every environment" cannot be literally true. Until it exists, the
commit SHA is the identity, and it must be recorded at every gate.
